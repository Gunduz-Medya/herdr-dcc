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
