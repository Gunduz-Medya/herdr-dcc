import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { theme, exec, appHome, readTextFile } from "herdr";

// Context use and account quota, exactly as the agent's status line reported them. Two separate
// parts. Nothing is calculated here: a missing source says "Unavailable", a null percentage says
// "bilinmiyor" (unknown), and a quota sample whose reset time has passed is marked, not replaced.
// The history is the stored samples drawn as they are, from the time DCC was running only.

const REFRESH_MS = 4000;
const BARS = "▁▂▃▄▅▆▇█";
const short = (s, n) => (String(s).length > n ? `${String(s).slice(0, n - 3)}...` : String(s));
const heading = (t, text) => div().text_size(11).font_semibold().text_color(t.muted).child(text);
const WINDOW_NAMES = { five_hour: "5 hour", seven_day: "7 day", spend_limit: "Spend limit" };

const pct = (v) => (Number.isInteger(v) ? `${v}%` : `${v.toFixed(1)}%`);
const when = (ms) => new Date(ms).toLocaleString();
function ago(ms, now) {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  return s < 60 ? `${s}s ago` : s < 3600 ? `${Math.floor(s / 60)}m ago` : `${Math.floor(s / 3600)}h ago`;
}
// each stored sample becomes one block, the block is a rounded picture of that stored value
function spark(samples) {
  return samples.slice(-40).map((s) => (s.used_percentage == null ? "·" : BARS[Math.max(0, Math.min(7, Math.floor((s.used_percentage / 100) * 8)))])).join("");
}

export default class DccUsage extends View {
  init(_props, cx) {
    this.cfg = null;
    this.doc = null;
    this.hist = {};          // window -> samples
    this.err = null;
    this.busy = false;
    this.last = "";
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-usage/config.json`));
      } catch (_e) {
        this.err = "config.json is missing, run dcc herdr ensure-mods";
        cx.notify();
        return;
      }
      this.refresh(cx);
    });
    cx.timer.every(REFRESH_MS, (cx) => this.refresh(cx));
  }

  dcc(args) {
    return exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, ...args], { timeout: 8000 });
  }

  refresh(cx) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
    cx.spawn(async (cx) => {
      let err = null;
      try {
        const r = await this.dcc(["usage", "show"]);
        if (r.code !== 0) throw new Error(`exit ${r.code}`);
        this.doc = JSON.parse(r.stdout);
        for (const w of ["five_hour", "seven_day"]) {
          const h = await this.dcc(["usage", "history", "--kind", "quota", "--window", w, "--hours", "24", "--limit", "120"]);
          if (h.code === 0) this.hist[w] = JSON.parse(h.stdout).samples;
        }
      } catch (e) { err = `usage failed, ${String(e?.message ?? e).slice(0, 80)}`; }
      this.busy = false;
      this.err = err;
      const sig = JSON.stringify([this.doc?.context, this.doc?.quota, this.hist, err]);
      if (sig !== this.last) { this.last = sig; cx.notify(); }
    });
  }

  contextRow(t, c, now) {
    let text; let color = t.foreground;
    if (c.status === "known") text = pct(c.used_percentage);
    else if (c.status === "unknown") { text = "bilinmiyor"; color = t.muted; }
    else { text = "Unavailable"; color = t.dim; }
    return v_flex().id(`ctx-${c.run_id}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
      .child(h_flex().gap(8)
        .child(div().flex_1().truncate().text_color(t.foreground).child(`${c.agent} ${String(c.run_id).slice(0, 8)}`))
        .child(div().font_semibold().text_color(color).child(text)))
      .children(c.sampled_ms ? [div().text_color(t.dim).child(`sampled ${ago(c.sampled_ms, now)}`)] : [])
      .children(c.reason ? [div().text_color(t.dim).child(c.reason)] : []);
  }

  quotaRow(t, w, now) {
    const name = WINDOW_NAMES[w.window] ?? w.window;
    let text; let color = t.foreground;
    if (w.status === "known") text = pct(w.used_percentage);
    else if (w.status === "unknown") { text = "bilinmiyor"; color = t.muted; }
    else { text = "Unavailable"; color = t.dim; }
    const h = this.hist[w.window] ?? [];
    return v_flex().id(`q-${w.window}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
      .child(h_flex().gap(8)
        .child(div().flex_1().text_color(t.foreground).child(name))
        .child(div().font_semibold().text_color(color).child(text)))
      .children(w.sampled_ms ? [div().text_color(t.dim).child(`sampled ${ago(w.sampled_ms, now)}`)] : [])
      .children(w.resets_at_ms ? [div().text_color(t.muted).child(`resets ${when(w.resets_at_ms)}`)] : [])
      .children(w.reset_passed ? [div().text_color(t.blocked).child("the reset time has passed, this is the last reported value")] : [])
      .children(h.length > 1 ? [div().text_color(t.muted).child(spark(h)), div().text_color(t.dim).child(`${h.length} samples in 24 h, only while DCC ran`)] : []);
  }

  render(_cx) {
    const t = theme();
    const now = Date.now();
    let body = v_flex().id("dcc-usage-body").flex_1().min_h(0).overflow_y_scrollbar().p(12).gap(8).text_size(12)
      .child(div().text_size(15).font_semibold().text_color(t.foreground).child("DCC usage"));
    if (this.err) body = body.child(div().text_color(t.blocked).child(this.err));
    if (this.doc) {
      body = body.child(heading(t, "CONTEXT (per agent session)"));
      if (this.doc.context.length === 0) body = body.child(div().text_color(t.muted).child("No agent is open."));
      for (const c of this.doc.context) body = body.child(this.contextRow(t, c, now));
      body = body.child(heading(t, "ACCOUNT QUOTA (from the status line)"));
      for (const w of this.doc.quota.windows) body = body.child(this.quotaRow(t, w, now));
      body = body.child(div().text_color(t.dim).child("Other agents: Unavailable (no official source found)"));
    } else if (!this.err) {
      body = body.child(div().text_color(t.dim).child("loading"));
    }
    return v_flex().id("dcc-usage").size_full().min_h(0).bg(t.surface).child(body);
  }
}
