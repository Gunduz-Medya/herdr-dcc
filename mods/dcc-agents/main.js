import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { theme, exec, appHome, readTextFile } from "herdr";

// Read only agent panel. Everything comes from `dcc agent snapshot`: the core (status, exit code),
// git (branch, worktree, changes) and the records DCC keeps (sessions, last hook, ended runs).
// The mod writes nothing. It polls every 2 s and redraws only when what it shows changed.

const REFRESH_MS = 2000;
const ORDER = ["Waiting", "Running", "Unknown", "Completed", "Stopped", "Failed"];

const NOTES = {
  no_status_claude: "güven sorusu olabilir veya henüz başlıyor",
  status_unreliable: "durum kanalı doğrulanamadı (sözleşme testi başarısız)",
  needs_permission: "izin bekliyor",
  ready_for_input: "girdi bekliyor",
  working: "çalışıyor",
  turn_done: "tur bitti",
  exited_ok: "çıkış kodu 0",
  exit_nonzero: "hata koduyla çıktı",
  killed: "sonlandırıldı",
  lost: "terminal kayboldu",
  agent_exited: "agent çıktı",
  no_exit_code: "çıkış kodu yok",
};

const hhmmss = (ms) => new Date(ms).toTimeString().slice(0, 8);

function ago(ms, now) {
  if (ms == null) return null;
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  return `${Math.floor(s / 3600)}h ago`;
}

const heading = (t, text) => div().text_size(11).font_semibold().text_color(t.muted).child(text);

export default class DccAgents extends View {
  init(_props, cx) {
    this.cfg = null;
    this.doc = null;        // last snapshot document
    this.err = null;
    this.busy = false;
    this.lastShown = "";
    this.updated = null;
    this.stats = { last: null, max: 0, sum: 0, n: 0 };
    cx.spawn(async (cx) => {
      try {
        this.cfg = JSON.parse(await readTextFile(`${appHome()}/mods/dcc-agents/config.json`));
      } catch (_e) {
        this.err = "config.json is missing, run dcc herdr ensure-mods";
        cx.notify();
        return;
      }
      this.refresh(cx);
    });
    cx.timer.every(REFRESH_MS, (cx) => this.refresh(cx));
  }

  refresh(cx) {
    if (!this.cfg || this.busy) return;
    this.busy = true;
    cx.spawn(async (cx) => {
      let err = null;
      try {
        const t0 = Date.now();
        const r = await exec(this.cfg.dcc, ["--data-dir", this.cfg.data_dir, "agent", "snapshot"], { timeout: 8000 });
        const ms = Date.now() - t0;
        this.stats.last = ms; this.stats.max = Math.max(this.stats.max, ms); this.stats.sum += ms; this.stats.n += 1;
        if (r.code !== 0) {
          let msg = `exit ${r.code}`;
          try { const e = JSON.parse(r.stderr).error; msg = `${e.kind}: ${e.message}`; } catch (_e) { /* keep exit code */ }
          throw new Error(msg);
        }
        this.doc = JSON.parse(r.stdout);
        this.updated = Date.now();
      } catch (e) {
        err = `snapshot failed: ${String(e?.message ?? e).slice(0, 90)}`;
      }
      this.busy = false;
      this.err = err;
      const now = Date.now();
      const sig = JSON.stringify([this.doc?.runs?.map((x) => [x.run_id, x.dcc_state, x.herdr_status, x.session_id, x.session_count, x.git?.branch, x.git?.worktree_kind, x.git?.changed_files, x.git?.staged, x.git?.unstaged, x.git?.untracked, x.last_event, x.shared_folder, Math.floor((now - (x.last_event_ms ?? now)) / 5000)]), this.doc?.warnings, this.doc?.status_reliable, err, this.stats.last]);
      if (sig !== this.lastShown) { this.lastShown = sig; cx.notify(); }
    });
  }

  dot(t, run) {
    if (run.dcc_state === "Running") return t.working;
    if (run.dcc_state === "Waiting") return run.herdr_status === "blocked" ? t.blocked : t.idle ?? t.dim;
    if (run.dcc_state === "Completed") return t.done;
    if (run.dcc_state === "Failed") return t.blocked;
    return t.dim;
  }

  row(t, run, now) {
    const g = run.git ?? {};
    const open = run.ended_ms == null;
    const id = run.session_id ? `${String(run.session_id).slice(0, 8)} (${run.session_id_source})${run.session_count > 1 ? `, ${run.session_count} sessions` : ""}` : "none";
    const branch = g.ok ? `${g.branch ?? "?"}` : "no git";
    const wt = g.ok ? (g.worktree_kind === "linked" ? "linked worktree" : "main worktree") : "-";
    const changes = g.ok && g.changed_files != null ? `${g.changed_files} changed (staged ${g.staged}, unstaged ${g.unstaged}, new ${g.untracked})` : "changes: n/a";
    const hook = run.last_event ? `${run.last_event} ${ago(run.last_event_ms, now)}` : "no hook";
    let note = NOTES[run.note_code] ?? run.note_code;
    if (run.note_code === "exit_nonzero" && run.exit_code != null) note = `${note} (${run.exit_code})`;
    const where = open ? [run.space, run.tab].filter(Boolean).join("/") : `ended ${ago(run.ended_ms, now)}`;
    return v_flex().id(`run-${run.run_id}`).gap(2).p(8).rounded(6).border(1).border_color(t.border)
      .child(h_flex().gap(8).items_center()
        .child(div().w(10).h(10).rounded(5).bg(this.dot(t, run)))
        .child(div().font_semibold().text_color(t.foreground).child(run.dcc_state))
        .child(div().text_color(t.dim).child(open ? `herdr: ${run.herdr_status ?? "no status"}` : run.end_reason ?? ""))
        .child(div().flex_1().truncate().text_color(t.dim).child(where)))
      .child(h_flex().gap(8)
        .child(div().text_color(t.foreground).truncate().child(branch))
        .child(div().text_color(t.muted).child(wt)))
      .child(div().text_color(t.muted).truncate().child(`session id: ${id}`))
      .child(div().text_color(t.muted).truncate().child(changes))
      .child(div().text_color(t.muted).truncate().child(`hook: ${hook}`))
      .children(note ? [div().text_color(run.dcc_state === "Unknown" ? t.blocked : t.dim).child(note)] : [])
      .children(run.shared_folder ? [div().text_color(t.blocked).child("aynı klasörde birden çok agent, değişiklikler ayrılamaz")] : []);
  }

  render(_cx) {
    const t = theme();
    const now = Date.now();
    let body = v_flex().id("dcc-agents-body").flex_1().min_h(0).overflow_y_scrollbar().p(12).gap(8).text_size(12)
      .child(div().text_size(15).font_semibold().text_color(t.foreground).child("DCC agents"));
    if (this.err) body = body.child(div().text_color(t.blocked).child(this.err));
    for (const w of this.doc?.warnings ?? []) body = body.child(div().text_color(t.blocked).child(w));
    if (this.doc === null && !this.err) {
      body = body.child(div().text_color(t.dim).child("loading"));
    } else if (this.doc && this.doc.runs.length === 0) {
      body = body.child(div().text_color(t.muted).child("No agents yet. Start one with dcc agent launch, or run Claude Code in any pane."));
    } else if (this.doc) {
      const rank = (r) => ORDER.indexOf(r.dcc_state) * 2 + (r.herdr_status === "blocked" ? 0 : 1);
      const open = this.doc.runs.filter((r) => r.ended_ms == null).sort((a, b) => rank(a) - rank(b) || String(a.run_id).localeCompare(String(b.run_id)));
      const ended = this.doc.runs.filter((r) => r.ended_ms != null);
      for (const r of open) body = body.child(this.row(t, r, now));
      if (ended.length) body = body.child(heading(t, "ENDED")).children(ended.map((r) => this.row(t, r, now)));
    }
    const st = this.stats;
    return v_flex().id("dcc-agents").size_full().min_h(0).bg(t.surface)
      .child(body
        .child(div().h(4))
        .child(heading(t, "QUOTA AND CONTEXT"))
        .child(div().text_color(t.muted).child("Kota: Unavailable (resmi kaynak yok)"))
        .child(div().text_color(t.dim).child("Context use: not shown"))
        .child(div().h(4))
        .child(div().text_color(t.dim).child(`updated ${this.updated ? hhmmss(this.updated) : "-"}  every ${REFRESH_MS / 1000}s`))
        .child(div().text_color(t.dim).child(st.n ? `snapshot ${st.last} ms (avg ${Math.round(st.sum / st.n)}, max ${st.max}, n ${st.n})` : "snapshot: no call yet")));
  }
}
