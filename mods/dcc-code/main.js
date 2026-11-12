import { View, div } from "gpui-kit";
import { focused, hc, openOverlay, notify, nextCommand, exec, appHome, readTextFile } from "herdr";

// Commands only, no panel. Bind them to keys in Herdr's own key settings.
//   dcc-code.lazygit-popup   lazygit in a window overlay, in the folder of the focused terminal
//   dcc-code.code-tab        the "code" workspace template (neovim, lazygit, shell) in that folder
// The mod has no diff, stage or commit screen of its own, lazygit does that. DCC installs nothing,
// a missing tool is reported with the command that installs it.

const say = (body, level = "info") => notify({ title: "dcc-code", body, level, timeout: level === "info" ? 3000 : 8000 });

export default class DccCode extends View {
  init(_props, cx) {
    this.cfg = null;
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-code/config.json`));
      } catch (_e) {
        say("config.json is missing, run dcc herdr ensure-mods", "error");
      }
    });
    cx.spawn(async (cx) => {
      for (;;) {
        const c = String(await nextCommand());
        if (c.endsWith("lazygit-popup")) await this.popup();
        else if (c.endsWith("code-tab")) await this.codeTab();
      }
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
    const r = await hc(["run", "--bg", "--json", "--name", "dcc-lazygit", "--cwd", cwd, "lazygit"]);
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

  render(_cx) { return div(); }
}
