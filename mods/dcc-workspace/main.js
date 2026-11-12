import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { spaces, focused, theme, exec, appHome, readTextFile, nextCommand } from "herdr";

// Saved workspaces. The mod only draws and calls dcc. `dcc herdr ensure-mods` writes config.json
// next to this file with the dcc binary and the data folder (exec has no environment).
// Every dcc call here is short, the 30 s exec limit never applies.

const REFRESH_MS = 5000;

const short = (s, n) => (String(s).length > n ? `${String(s).slice(0, n - 3)}...` : String(s));

function button(t, id, label, onClick) {
  return div().id(id).px(8).py(4).rounded(6).border(1).border_color(t.border)
    .hover((st) => st.bg(t.raised)).text_color(t.foreground).text_size(12)
    .on_click(onClick).child(label);
}

export default class DccWorkspace extends View {
  init(_props, cx) {
    this.cfg = null;        // { dcc, data_dir }
    this.items = null;      // saved definitions, or { error }
    this.note = null;       // result of the last action
    this.busy = false;
    this.last = "";
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-workspace/config.json`));
      } catch (_e) {
        this.items = { error: "config.json is missing, run dcc herdr ensure-mods" };
        cx.notify();
        return;
      }
      this.refresh(cx);
    });
    cx.timer.every(REFRESH_MS, (cx) => this.refresh(cx));
    cx.spawn(async (cx) => {
      for (;;) {
        const c = String(await nextCommand());
        if (c.endsWith("save-focused")) this.saveFocused(cx);
      }
    });
  }

  dcc(args) {
    return exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, ...args], { timeout: 20000 });
  }

  refresh(cx) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
    cx.spawn(async (cx) => {
      try {
        const r = await this.dcc(["workspace", "list"]);
        this.items = r.code === 0 ? JSON.parse(r.stdout).workspaces : { error: this.errorText(r) };
      } catch (e) {
        this.items = { error: String(e?.message ?? e).slice(0, 80) };
      }
      this.busy = false;
      const sig = JSON.stringify([this.items, this.note, spaces().map((s) => s.name)]);
      if (sig !== this.last) { this.last = sig; cx.notify(); }
    });
  }

  errorText(r) {
    try { const e = JSON.parse(r.stderr).error; return `${e.kind}: ${e.message}`.slice(0, 120); } catch (_e) { return `exit ${r.code}`; }
  }

  act(cx, label, args) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
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

  saveFocused(cx) {
    const f = focused();
    const sp = spaces().find((s) => s.id === f?.space);
    if (!sp) { this.note = "no focused space"; cx.notify(); return; }
    this.act(cx, `save ${sp.name}`, ["workspace", "save", sp.id]);
  }

  render(_cx) {
    const t = theme();
    const open = new Set(spaces().map((s) => s.name));
    let body = v_flex().id("dcc-ws-body").flex_1().min_h(0).overflow_y_scrollbar().p(12).gap(8).text_size(12)
      .child(div().text_size(15).font_semibold().text_color(t.foreground).child("DCC workspaces"))
      .child(button(t, "save-focused", "Save the focused space", (_e, cx) => this.saveFocused(cx)));
    if (this.items === null) {
      body = body.child(div().text_color(t.dim).child("loading"));
    } else if (this.items.error) {
      body = body.child(div().text_color(t.blocked).child(this.items.error));
    } else if (this.items.length === 0) {
      body = body.child(div().text_color(t.muted).child("Nothing saved yet. Focus a space and press Save."));
    } else {
      for (const w of this.items) {
        const isOpen = open.has(w.name);
        const label = isOpen ? "Rehydrate" : "Restore";
        const args = ["workspace", isOpen ? "rehydrate" : "restore", w.name];
        body = body.child(v_flex().id(`ws-${w.name}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
          .child(h_flex().gap(8).items_center()
            .child(div().flex_1().truncate().font_semibold().text_color(t.foreground).child(w.name))
            .child(div().text_color(t.dim).child(isOpen ? "open" : w.kind)))
          .child(div().truncate().text_color(t.muted).child(short(w.folder, 44)))
          .child(h_flex().child(button(t, `act-${w.name}`, label, (_e, cx) => this.act(cx, `${label} ${w.name}`, args)))));
      }
    }
    if (this.note) body = body.child(div().text_color(t.muted).child(short(this.note, 140)));
    return v_flex().id("dcc-workspace").size_full().min_h(0).bg(t.surface).child(body);
  }
}
