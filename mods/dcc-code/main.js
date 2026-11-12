import { View, div } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focused, hc, openOverlay, notify, nextCommand, exec, appHome, readTextFile, theme } from "herdr";

// A small panel (the focused folder, its branch and changes, two buttons) that also keeps the two
// commands below loaded, because Herdr only knows a mod's commands while the mod is open.
// Bind the commands to keys in Herdr's own key settings.
//   dcc-code.lazygit-popup   lazygit in a window overlay, in the folder of the focused terminal
//   dcc-code.code-tab        the "code" workspace template (neovim, lazygit, shell) in that folder
// The mod has no diff, stage or commit screen of its own, lazygit does that. DCC installs nothing,
// a missing tool is reported with the command that installs it.

const say = (body, level = "info") => notify({ title: "dcc-code", body, level, timeout: level === "info" ? 3000 : 8000 });

export default class DccCode extends View {
  init(_props, cx) {
    this.cfg = null;
    this.git = null;      // `dcc git info` of the focused folder
    this.gitCwd = null;
    this.tools = null;
    this.busy = false;
    this.last = "";
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-code/config.json`));
      } catch (_e) {
        say("config.json is missing, run dcc herdr ensure-mods", "error");
        return;
      }
      this.refresh(cx);
    });
    cx.timer.every(3000, (cx) => this.refresh(cx));
    cx.spawn(async (cx) => {
      for (;;) {
        const c = String(await nextCommand());
        if (c.endsWith("lazygit-popup")) await this.popup();
        else if (c.endsWith("code-tab")) await this.codeTab();
      }
    });
  }

  refresh(cx) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
    const cwd = focused()?.cwd ?? null;
    cx.spawn(async (cx) => {
      try {
        if (this.tools === null) {
          const t = await this.dcc(["git", "tools"]);
          if (t.code === 0) this.tools = JSON.parse(t.stdout).tools;
        }
        this.gitCwd = cwd;
        if (cwd) {
          const r = await this.dcc(["git", "info", cwd]);
          this.git = r.code === 0 ? JSON.parse(r.stdout) : null;
        } else {
          this.git = null;
        }
      } catch (_e) {
        this.git = null;
      }
      this.busy = false;
      const sig = JSON.stringify([this.git, this.gitCwd, this.tools]);
      if (sig !== this.last) { this.last = sig; cx.notify(); }
    });
  }

  dcc(args) {
    return exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, ...args], { timeout: 20000 });
  }

  async missing(tool) {
    if (!this.cfg) { say("config.json is missing, run dcc herdr ensure-mods", "error"); return true; }
    const r = await this.dcc(["git", "tools"]);
    if (r.code !== 0) { say("dcc git tools failed", "error"); return true; }
    const t = JSON.parse(r.stdout).tools.find((x) => x.name === tool);
    if (t && !t.path) { say(`${tool} is not installed. Install it with: ${t.install}`, "warning"); return true; }
    return false;
  }

  async popup() {
    const cwd = focused()?.cwd ?? null;
    if (!cwd) { say("no focused folder", "warning"); return; }
    if (await this.missing("lazygit")) return;
    const r = await hc(["run", "--bg", "--json", "--name", `dcc-lazygit-${Date.now().toString(36)}`, "--cwd", cwd, "lazygit"]);
    if (r.code !== 0) { say(`could not start lazygit (${r.code})`, "error"); return; }
    const addr = JSON.parse(r.stdout).address;
    openOverlay({ kind: "window", key: "lazygit", width: 0.8, height: 0.7, layout: { type: "block", content: { terminal: addr } } });
  }

  async codeTab() {
    const cwd = focused()?.cwd ?? null;
    if (!cwd) { say("no focused folder", "warning"); return; }
    if (await this.missing("nvim") || await this.missing("lazygit")) return;
    const base = cwd.split("/").filter(Boolean).pop() ?? "code";
    const name = `code-${base}`.toLowerCase().replace(/[^a-z0-9._-]/g, "-").slice(0, 48);
    const r = await this.dcc(["workspace", "template", "apply", "code", "--name", name, "--folder", cwd]);
    if (r.code !== 0) {
      let msg = `exit ${r.code}`;
      try { const e = JSON.parse(r.stderr).error; msg = `${e.kind}: ${e.message}`; } catch (_e) { /* keep exit code */ }
      say(`code tab failed, ${msg}`.slice(0, 160), "error");
    }
  }

  render(cx) {
    const t = theme();
    const btn = (id, label, fn) => div().id(id).px(8).py(4).rounded(6).border(1).border_color(t.border)
      .hover((st) => st.bg(t.raised)).text_color(t.foreground).text_size(12).on_click(() => fn()).child(label);
    const g = this.git;
    const facts = !this.gitCwd ? "no focused folder"
      : !g || g.ok === false ? "not a git folder"
      : `${g.branch ?? "detached"}, ${g.worktree_kind === "linked" ? "linked worktree" : "main worktree"}, ${g.changed_files ?? 0} changed`;
    const missing = (this.tools ?? []).filter((x) => !x.path);
    return v_flex().id("dcc-code").size_full().min_h(0).bg(t.surface).p(12).gap(8).text_size(12)
      .child(div().text_size(15).font_semibold().text_color(t.foreground).child("DCC code"))
      .child(div().truncate().text_color(t.muted).child(this.gitCwd ?? "-"))
      .child(div().text_color(t.muted).child(facts))
      .child(btn("code-popup", "Lazygit popup", () => this.popup()))
      .child(btn("code-tab", "Open code tab", () => this.codeTab()))
      .children(missing.map((x) => div().text_color(t.blocked).child(`${x.name} missing: ${x.install}`)));
  }
}
