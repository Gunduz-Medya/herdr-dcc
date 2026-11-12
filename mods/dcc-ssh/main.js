import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { focused, theme, exec, appHome, readTextFile } from "herdr";

// SSH profiles, forwards and saved commands. The mod only draws and calls dcc. It never sees a
// password: "Set secret" opens a terminal tab with a hidden prompt (`dcc ssh profile secret-tab`).
// Everything comes from one call, `dcc ssh overview`, every 3 s.

const REFRESH_MS = 3000;
const short = (s, n) => (String(s).length > n ? `${String(s).slice(0, n - 3)}...` : String(s));

function button(t, id, label, onClick) {
  return div().id(id).px(8).py(4).rounded(6).border(1).border_color(t.border)
    .hover((st) => st.bg(t.raised)).text_color(t.foreground).text_size(12)
    .on_click(onClick).child(label);
}

const heading = (t, text) => div().text_size(11).font_semibold().text_color(t.muted).child(text);

export default class DccSsh extends View {
  init(_props, cx) {
    this.cfg = null;
    this.doc = null;
    this.err = null;
    this.note = null;
    this.busy = false;
    this.last = "";
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-ssh/config.json`));
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
    return exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, ...args], { timeout: 20000 });
  }

  errorText(r) {
    try { const e = JSON.parse(r.stderr).error; return `${e.kind}: ${e.message}`.slice(0, 140); } catch (_e) { return `exit ${r.code}`; }
  }

  refresh(cx) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
    cx.spawn(async (cx) => {
      let err = null;
      try {
        const r = await this.dcc(["ssh", "overview"]);
        if (r.code !== 0) throw new Error(this.errorText(r));
        this.doc = JSON.parse(r.stdout);
      } catch (e) {
        err = String(e?.message ?? e).slice(0, 120);
      }
      this.busy = false;
      this.err = err;
      const sig = JSON.stringify([this.doc, this.note, err]);
      if (sig !== this.last) { this.last = sig; cx.notify(); }
    });
  }

  act(cx, label, args) {
    if (!this.cfg) return;
    this.note = `${label}...`;
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        const r = await this.dcc(args);
        this.note = r.code === 0 ? `${label} done` : `${label} failed, ${this.errorText(r)}`;
      } catch (e) {
        this.note = `${label} failed, ${String(e?.message ?? e).slice(0, 80)}`;
      }
      this.busy = false;
      this.refresh(cx);
      cx.notify();
    });
  }

  runCommand(cx, name) {
    const terminal = focused()?.terminal;
    if (!terminal) { this.note = "no focused terminal, click a terminal first"; cx.notify(); return; }
    this.act(cx, `run ${name}`, ["ssh", "command", "run", name, "--terminal", String(terminal)]);
  }

  dot(t, state) {
    if (state === "running") return t.working;
    if (state === "failed") return t.blocked;
    return t.dim;
  }

  render(_cx) {
    const t = theme();
    let body = v_flex().id("dcc-ssh-body").flex_1().min_h(0).overflow_y_scrollbar().p(12).gap(8).text_size(12)
      .child(div().text_size(15).font_semibold().text_color(t.foreground).child("DCC SSH"));
    if (this.err) body = body.child(div().text_color(t.blocked).child(this.err));
    if (this.doc?.warning) body = body.child(div().text_color(t.blocked).child(short(this.doc.warning, 140)));
    if (!this.doc && !this.err) {
      body = body.child(div().text_color(t.dim).child("loading"));
    } else if (this.doc) {
      body = body.child(heading(t, "PROFILES"));
      if (this.doc.profiles.length === 0) body = body.child(div().text_color(t.muted).child("No profile yet. Add one with dcc ssh profile add."));
      for (const p of this.doc.profiles) {
        const needsSecret = p.auth !== "key";
        const missing = needsSecret && p.secret_stored === false;
        body = body.child(v_flex().id(`p-${p.name}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
          .child(h_flex().gap(8).items_center()
            .child(div().flex_1().truncate().font_semibold().text_color(t.foreground).child(p.name))
            .child(div().text_color(t.dim).child(p.group ?? "")))
          .child(div().truncate().text_color(t.muted).child(`${p.user}@${p.host}:${p.port}, ${p.auth}`))
          .children(missing ? [div().text_color(t.blocked).child(`${p.auth} not stored yet`)] : [])
          .child(h_flex().gap(6)
            .child(button(t, `c-${p.name}`, "Connect", (_e, cx) => this.act(cx, `connect ${p.name}`, ["ssh", "connect", p.name])))
            .children(needsSecret ? [button(t, `s-${p.name}`, missing ? "Set secret" : "Change secret", (_e, cx) => this.act(cx, `secret ${p.name}`, ["ssh", "profile", "secret-tab", p.name]))] : [])));
      }
      body = body.child(heading(t, "FORWARDS"));
      if (this.doc.forwards.length === 0) body = body.child(div().text_color(t.muted).child("No forward yet."));
      for (const f of this.doc.forwards) {
        const key = `${f.profile}/${f.name}`;
        const running = f.state === "running";
        const route = f.kind === "dynamic" ? `socks ${f.bind}` : `${f.kind} ${f.bind} to ${f.target}`;
        body = body.child(v_flex().id(`f-${key}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
          .child(h_flex().gap(8).items_center()
            .child(div().w(10).h(10).rounded(5).bg(this.dot(t, f.state)))
            .child(div().flex_1().truncate().font_semibold().text_color(t.foreground).child(key))
            .child(div().text_color(t.dim).child(f.state === "failed" && f.exit_code != null ? `failed (${f.exit_code})` : f.state)))
          .child(div().truncate().text_color(t.muted).child(route))
          .child(h_flex().child(button(t, `fa-${key}`, running ? "Stop" : "Start", (_e, cx) => this.act(cx, `${running ? "stop" : "start"} ${key}`, ["ssh", "forward", running ? "stop" : "start", f.profile, f.name])))));
      }
      body = body.child(heading(t, "COMMANDS"));
      if (this.doc.commands.length === 0) body = body.child(div().text_color(t.muted).child("No saved command yet."));
      for (const c of this.doc.commands) {
        body = body.child(v_flex().id(`k-${c.name}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
          .child(div().font_semibold().text_color(t.foreground).child(c.name))
          .child(div().truncate().text_color(t.muted).child(short(c.command, 48)))
          .child(h_flex().child(button(t, `kr-${c.name}`, "Run in focused terminal", (_e, cx) => this.runCommand(cx, c.name)))));
      }
    }
    if (this.note) body = body.child(div().text_color(t.muted).child(short(this.note, 160)));
    return v_flex().id("dcc-ssh").size_full().min_h(0).bg(t.surface).child(body);
  }
}
