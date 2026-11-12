# İlerleme kaydı

Her bölüm bir faz kaydıdır. Her kayıtta faz, tarih, ne yapıldı, test sonucu ve açık sorun yer alır.

## Faz 1, dokümanlar (2026-11-12)

- Ne yapıldı. `docs/01-capability-inventory.md` (envanter, 13 düzeltmeyle), `docs/02-PRD.md`, `docs/03-architecture.md` (Mermaid diyagramı, SQLite şeması, adapter arayüzü), `docs/04-comparative-analysis.md` (web kaynakları, erişim 2026-11-12), `docs/05-roadmap.md` (Faz 0 ile 7) yazıldı. `docs/CLEANUP.md` açıldı.
- Test sonucu. Kod yok, test yok. Dokümanlarda yasaklı karakter ve sözcük taraması yapıldı, temiz çıktı.
- Açık sorun.
  - Görev metni `docs/inputs/DCC-INVENTORY.md` diyordu, gerçek dosya `docs/inputs/DCC Yetenek Envanteri.md`. Aynı dosya kabul edildi, dokunulmadı.
  - TermSplit için resmi kaynak bulunamadı (iki arama). Karşılaştırma yapılamadı.
  - Herdr masaüstü uygulaması ve core lisansı doğrulanmadı. TUI deposu için iki kaynak çelişiyor (Apache 2.0 ve AGPL-3.0-or-later).
  - Brifteki "global yazı kuralları dosyalarını oku" (`~/.claude`) maddesi uygulanmadı, çünkü görev kuralları `~/.claude` altını okumayı yasaklıyor. Yazım kuralları görev metnindeki listeye göre uygulandı.
  - İlk (boş) commit `main` üzerinde, çünkü dallanmak için bir kök commit gerekiyor. Dokümanlar `phase-1-docs` branch'inde.
  - Durma noktası 1. Kullanıcı onayı bekleniyor.

## Faz 0, doğrulama testleri (2026-11-12)

- Ne yapıldı. Beş test izole ortamda (`HC_SESSION=dcc-dev`, `~/dev/dcc/.dev`) yürütüldü. Sonuçlar `docs/06-validation.md` içinde.
  1. `claude --settings` ile hook ve statusline geçti. Ek bulgu, `/clear` yeni oturum kimliği üretiyor.
  2. Lazygit ve neovim pane'de, lazygit popup'ı geçti. Popup için modun yüklü olması gerekiyor.
  3. Kod sekmesi geçti (koşullu). Şablon bir komut dizisi, Herdr yapıyı kendisi koruyor, programlar kayboluyor.
  4. SQLite oturum kaydı geçti (koşullu). Çıkış kodu yalnızca `hc run` ile açılan terminalde var.
  5. SSH geçti (yalnızca localhost). Üç yönlendirme türü ve `hc machine add --via` çalıştı. Çekirdek soket yolu 103 bayt ile sınırlı.
- Mimariye etki. `03-architecture.md` (çalıştırma ve oturum ayrımı, `hc run` ile başlatma, workspace iki mod, mod yükleme, soket yolu), `05-roadmap.md` (Faz 1, 2 ve 3 görevleri), `02-PRD.md` (açık sorular) güncellendi.
- Test sonucu. `~/.claude/settings.json` özeti ve zamanı değişmedi. Test süreçleri PID ile doğrulanmış kapalı.
- Açık sorun.
  - Test 3'te core yeniden başlatması, Test 1'in güven sorusunda bekleyen Claude sürecini öldürdü, onay iki kez istendi.
  - Uzak çekirdeği durdururken ortam değişkenlerini zsh'te bölünmeyen bir dizgiyle verdim, yanlış çekirdek durdu. Doğru çekirdek ayrıca durduruldu.
  - `--settings` ile kullanıcı ayar kaynağı açıkken davranış, gerçek Claude'un `hc run` ile açılması, popup modunun kalıcı yüklenmesi, özel bölme oranları, gerçek uzak makine denenmedi.
  - Durma noktası 2. Kullanıcı onayı bekleniyor.
