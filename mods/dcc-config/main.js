import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { theme, exec, appHome, readTextFile } from "herdr";

// AI tool configuration. Part one is the inventory (paths, kinds and sizes, never file content),
// part two is Skillshare: a preview of what a sync would do, and an apply that needs that preview.
// A sync that would remove something needs a second, separate button. The apply runs as a job.

const REFRESH_MS = 8000;
const fmt = (n) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);
const short = (s, n) => (String(s).length > n ? `${String(s).slice(0, n - 3)}...` : String(s));
const heading = (t, text) => div().text_size(11).font_semibold().text_color(t.muted).child(text);

function button(t, id, label, onClick, warn = false) {
  return div().id(id).px(8).py(4).rounded(6).border(1).border_color(warn ? t.blocked : t.border)
    .hover((st) => st.bg(t.raised)).text_color(warn ? t.blocked : t.foreground).text_size(12)
    .on_click(onClick).child(label);
}

export default class DccConfig extends View {
  init(_props, cx) {
    this.cfg = null;
    this.inv = null;
    this.preview = null;   // result of `config sync preview`
    this.job = null;       // { id, state }
    this.note = null;
    this.err = null;
    this.busy = false;
    this.last = "";
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-config/config.json`));
      } catch (_e) {
        this.err = "config.json is missing, run dcc herdr ensure-mods";
        cx.notify();
        return;
      }
      this.refresh(cx);
    });
    cx.timer.every(REFRESH_MS, (cx) => { this.refresh(cx); this.pollJob(cx); });
  }

  dcc(args, timeout = 20000) {
    const home = this.cfg.ai_home ? ["--home", this.cfg.ai_home] : [];
    const [first, ...rest] = args;
    const withHome = first === "config" ? ["config", ...home, ...rest] : args;
    return exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, ...withHome], { timeout });
  }

  errorText(r) {
    try { const e = JSON.parse(r.stderr).error; return `${e.kind}: ${e.message}`.slice(0, 160); } catch (_e) { return `exit ${r.code}`; }
  }

  refresh(cx) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
    cx.spawn(async (cx) => {
      let err = null;
      try {
        const r = await this.dcc(["config", "inventory"]);
        if (r.code !== 0) throw new Error(this.errorText(r));
        this.inv = JSON.parse(r.stdout);
      } catch (e) { err = String(e?.message ?? e).slice(0, 140); }
      this.busy = false;
      this.err = err;
      const sig = JSON.stringify([this.inv?.agents, this.preview?.hash, this.job, this.note, err]);
      if (sig !== this.last) { this.last = sig; cx.notify(); }
    });
  }

  runPreview(cx) {
    if (!this.cfg) return;
    this.note = "previewing...";
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        const r = await this.dcc(["config", "sync", "preview"], 25000);
        if (r.code === 0) { this.preview = JSON.parse(r.stdout); this.note = null; }
        else { this.preview = null; this.note = this.errorText(r); }
      } catch (e) { this.note = `preview failed, ${String(e?.message ?? e).slice(0, 100)}`; }
      cx.notify();
    });
  }

  apply(cx, allowDelete) {
    if (!this.cfg || !this.preview) return;
    const args = ["job", "start", "--", "config", ...(this.cfg.ai_home ? ["--home", this.cfg.ai_home] : []), "sync", "apply", "--preview-hash", this.preview.hash, ...(allowDelete ? ["--allow-delete"] : [])];
    this.note = "starting the sync job...";
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        const r = await exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, ...args], { timeout: 20000 });
        if (r.code === 0) { this.job = { id: JSON.parse(r.stdout).id, state: "running" }; this.note = null; }
        else this.note = this.errorText(r);
      } catch (e) { this.note = `could not start the job, ${String(e?.message ?? e).slice(0, 100)}`; }
      cx.notify();
    });
  }

  pollJob(cx) {
    if (!this.cfg || !this.job || this.job.state !== "running") return;
    cx.spawn(async (cx) => {
      try {
        const r = await exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, "job", "status", this.job.id], { timeout: 10000 });
        const s = JSON.parse(r.stdout);
        if (s.state !== "running") {
          this.job = { id: this.job.id, state: s.state, code: s.exit_code ?? null };
          this.preview = null;
          this.refresh(cx);
        }
      } catch (_e) { /* keep polling */ }
      cx.notify();
    });
  }

  render(_cx) {
    const t = theme();
    let body = v_flex().id("dcc-config-body").flex_1().min_h(0).overflow_y_scrollbar().p(12).gap(8).text_size(12)
      .child(div().text_size(15).font_semibold().text_color(t.foreground).child("DCC AI config"));
    if (this.err) body = body.child(div().text_color(t.blocked).child(this.err));
    body = body.child(heading(t, "INVENTORY (paths and sizes only)"));
    if (this.inv) {
      if (this.inv.agents.length === 0) body = body.child(div().text_color(t.muted).child("No AI tool configuration found."));
      for (const a of this.inv.agents) {
        const kinds = Object.entries(a.by_kind).map(([k, n]) => `${n} ${k}`).join(", ");
        body = body.child(v_flex().id(`a-${a.agent}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
          .child(h_flex().gap(8)
            .child(div().flex_1().font_semibold().text_color(t.foreground).child(a.agent))
            .child(div().text_color(t.dim).child(fmt(a.bytes))))
          .child(div().text_color(t.muted).child(short(kinds, 60))));
      }
      if (this.inv.withheld_credential_like > 0) body = body.child(div().text_color(t.dim).child(`${this.inv.withheld_credential_like} credential like file(s) not listed`));
    }
    body = body.child(heading(t, "SKILLSHARE SYNC"))
      .child(h_flex().child(button(t, "preview", "Preview sync", (_e, cx) => this.runPreview(cx))));
    const p = this.preview;
    if (p) {
      const s = p.summary;
      body = body.child(v_flex().gap(2).p(8).rounded(6).border(1).border_color(t.border)
        .child(div().text_color(t.foreground).child(`would add ${s.add}, update ${s.update}, remove ${s.remove}`))
        .child(div().text_color(t.dim).child("nothing written yet, a backup is made before the sync")));
      for (const t0 of p.diff.targets ?? []) {
        for (const i of (t0.items ?? []).slice(0, 12)) {
          body = body.child(div().truncate().text_color(i.action === "remove" && i.is_sync ? t.blocked : t.muted).child(`${t0.name}: ${i.action} ${i.kind} ${i.name}`));
        }
      }
      const running = this.job?.state === "running";
      if (s.remove > 0) {
        body = body.child(div().text_color(t.blocked).child("This sync removes items. Removal needs its own approval."))
          .child(h_flex().child(button(t, "apply-del", "Approve removal and apply", (_e, cx) => this.apply(cx, true), true)));
      } else if (s.add + s.update > 0) {
        body = body.child(h_flex().child(button(t, "apply", running ? "Sync running" : "Back up and apply", (_e, cx) => { if (!running) this.apply(cx, false); })));
      } else {
        body = body.child(div().text_color(t.muted).child("Nothing to sync."));
      }
    }
    if (this.job) body = body.child(div().text_color(t.muted).child(`sync job ${short(this.job.id, 14)}: ${this.job.state}${this.job.code != null ? ` (${this.job.code})` : ""}`));
    if (this.note) body = body.child(div().text_color(t.muted).child(short(this.note, 200)));
    return v_flex().id("dcc-config").size_full().min_h(0).bg(t.surface).child(body);
  }
}
