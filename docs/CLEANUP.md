# Temizlik listesi

Hiçbir şey otomatik silinmez. Silinmesi gerekenler yol yol burada tutulur, silme kararı kullanıcıdadır.

## Bu depodan

Faz 0 doğrulama testlerinden kalanlar (hiçbiri silinmedi).

- `~/dev/dcc/.dev/` klasörü. Git'e girmiyor. İçinde test repoları (`t1-repo`, `t2-repo`, `t2-repo-b`), deneme betikleri, loglar, ekran görüntüleri, scratch Herdr klasörleri (`scratch-home`, `scratch-xdg`, `scratch-data`, `rx`), test sshd anahtarları ve yapılandırması (`ssh/`, yalnızca bu test için üretildi) ve `settings.before.sha` var.
- `~/.claude.json` içinde `~/dev/dcc/.dev/t1-repo` için Claude Code'un yazdığı güven kaydı.
- `~/.claude/projects/-Users-barisgunduz-dev-dcc--dev-t1-repo` klasörü (Claude Code'un yazdığı oturum dosyaları). Adı kural gereği bu çalışmada listelenmedi, silmeden önce `~/.claude/projects` altında listeleyip doğrula.

Faz 1'den kalanlar (hiçbiri silinmedi).

- `~/dev/dcc/.dev/phase0/` (Faz 0'ın scratch klasörleri, yeni SQLite şemasıyla çakışmasın diye taşındı), `~/dev/dcc/.dev/ws-proj/`, `~/dev/dcc/.dev/rust/` (spike'tan kopyalanan Rust araç zinciri, yaklaşık 450 MB, `.dev` içinde), `~/dev/dcc/.dev/job-probe-*.json` ve `~/dev/dcc/.dev/p1-code-template.png`.
- `~/dev/dcc/target/test-tmp/` (cargo testlerinin geçici klasörleri, `target` içinde).
- Bu fazda Claude Code oturumu açılmadı, `~/.claude` altında yeni bir şey oluşmadı.

Not. Faz 0 sonunda tüm test süreçleri (uygulama, iki çekirdek, holder'lar, sshd, http sunucusu) PID ile doğrulanmış kapalı.

## Spike klasöründen (`SPIKE-SUMMARY.md` bölüm 8'den aktarıldı, silinmedi)

Silinecekler adayları.

- `~/dev/dcc-spike/agent-temp-nohooks`
- `~/dev/dcc-spike/dcc/target/test-tmp/` içindeki 12 test klasörü
- `~/dev/dcc-spike/agent-playground`
- `~/dev/dcc-spike/agent-playground-wt`
- `~/.claude/projects/-Users-barisgunduz-dev-dcc-spike-agent-sandbox` (Claude Code'un yazdığı oturum dosyaları)
- `~/.claude/projects/-Users-barisgunduz-dev-dcc-spike-agent-sandbox-wt1`
- `~/.claude/projects/-Users-barisgunduz-dev-dcc-spike-agent-temp-nohooks`
- `~/.claude/projects/-Users-barisgunduz-dev-dcc-spike-agent-playground*` (ilk geçiş)
- Keychain'de `dcc-spike` servisi altındaki `prompt-token` kaydı, silinip silinmediği doğrulanmadı

Not. `~/.claude` altındaki yollar yalnızca liste olarak yazıldı, bu depodaki çalışma sırasında bu dizine dokunulmadı.

Spike'ta tutulanlar (Adım 7 sonrası için). `agent-sandbox` ve `agent-sandbox-wt1` repoları, `logs/`, `scratch-home`, `scratch-xdg`, `scratch-data`, `toolchain`.

Faz 2 testlerinden kalanlar (silinmedi).
- `~/dev/dcc/.dev/p2-repo` ve `~/dev/dcc/.dev/p2-repo.worktrees/*` (test deposu ve agent worktree'leri, `dcc/*` branch'leri)
- `~/dev/dcc/.dev/scratch-data/agents/*` (agent ayar dosyaları)
- `~/dev/dcc/.dev/p2-panel.png`, `~/dev/dcc/.dev/p2-empty.png`
- `~/.claude/projects/-Users-barisgunduz-dev-dcc--dev-p2-repo.worktrees-08b85856` (gerçek Claude Code'un yazdığı oturum dosyası, DCC yazmadı)
