# Karşılaştırmalı analiz

Sürüm 0.1, taslak. Web kaynaklarına erişim tarihi 2026-11-12. Lisans notları hukuki tavsiye değildir. Kaynağı bulunamayan veya çelişen bilgi "doğrulanmadı" diye işaretlidir.

## 1. Sorular ve kısa cevap

DCC için üç yol vardı.

1. Warp'ı fork'lamak.
2. Herdr'ı özelleştirmek (modlar ve gerekirse çekirdek değişikliği).
3. Hibrit. Herdr olduğu gibi kalır, özellikler Herdr modları ve küçük bir Rust yardımcısı (dcc) ile yapılır.

**Karar hibrit (KK-01).** Gerekçe bölüm 4'te. Kısaca, spike'ta denenen her yetenek Herdr'a dokunmadan çalıştı, Herdr'ın tek başına yapamadığı işler (Keychain, SQLite, git, uzun işler) dcc ile çözüldü, Warp'ı fork'lamanın getireceği bakım yükü ve lisans yükümlülüğü için ise somut bir kazanç görülmedi.

## 2. Ürün ürün

### 2.1 Warp

- **Ne olduğu.** Kendini "agentic development environment" olarak tanıtan bir terminal. İstemci kodu GitHub'da açık (`warpdotdev/warp`).
- **Lisans.** Deponun sayfası, kodun büyük bölümünün AGPL v3, `warpui_core` ve `warpui` arayüz kütüphanelerinin MIT olduğunu söylüyor. Sayfa kapalı kalan bir bölüm saymıyor. Bir ikincil kaynak (haber ve blog özetleri) bulut agent platformunun (Oz) açık kaynak dışında kaldığını yazıyor, bu birincil kaynakta görülmedi ve doğrulanmadı.
- **Lisans notu.** AGPL v3 ile türetilmiş bir sürümü dağıtmak, aynı lisansla kaynak kodunu vermeyi ve ağ üzerinden kullanıma açıldığında da kaynağı sunmayı gerektirir. Bizim kararımız Warp kaynağından hiçbir kod kopyalamamaktır (KK-01).
- **Neyi alıyoruz.** Fikirler. Agent odaklı terminal deneyimi, komut blokları (sonraki sürüm), agent durumunun görünürlüğü. Referans olarak okunur.
- **Neyi almıyoruz.** Kod, fork, mimari kopya. Warp'ın kod büyüklüğü ve bakım maliyeti bu çalışmada ölçülmedi, kararımız o ölçüme dayanmıyor.

### 2.2 Herdr

- **Ne olduğu.** AI agent'lar için terminal çalışma alanı. Spike'ta iki parça ölçüldü. Masaüstü uygulaması (herdr-app, GPUI, sürüm 0.1.15) ve çekirdek (herdr-core, `hc`, sürüm 0.1.20). Terminaller çekirdekte yaşar, uygulama bunları sekme, split ve mod panelleri olarak gösterir. Mod arayüzü JavaScript.
- **Spike'ta kanıtlanan.** Mod ile dış program çağırma, layout kopyası, agent durumu (agent'ın kendi OSC 7501 raporu), workspace komutları (`space.new`, `tab.new`, `terminal.open`), salt okunur paneller. Hepsi `docs/01-capability-inventory.md` içinde.
- **Lisans.** Doğrulanmadı. Web araştırmasında yalnızca terminal (TUI) sürümünün deposu (`ogulcancelik/herdr`) bulundu. O sayfa Apache 2.0 diyor ve masaüstü uygulamasını ya da herdr-core'u anmıyor. Aynı depo için bir arama özeti AGPL-3.0-or-later yazdı, iki kaynak çelişiyor ve çözülmedi. Masaüstü uygulaması ve core için lisans bilgisi bulunamadı (PRD açık soru 1).
- **Sürüm.** Uygulama 0.1.x. API kararlılığı taahhüdü belgelerde görülmedi, bu doğrulanmadı.
- **Neyi alıyoruz.** Tamamını. Çalışma alanı, agent durumu, layout, mod sistemi, `hc`.
- **Neyi almıyoruz.** Çekirdek değişikliği ve fork. Gerekirse yalnızca pane boyutu kalıcılığı ve komut blokları için düşünülür (ikisi de MVP dışı).

### 2.3 TermSplit

- **Durum.** Resmi bir kaynak bulunamadı. "TermSplit" adıyla yapılan aramada bu isimde bir ürün sayfası, depo veya mağaza girişi çıkmadı. Yalnızca benzer işi yapan başka araçlar görüldü (Tabs, TermPolis, xplit, Termio) ve bunlar incelenmedi.
- **Sonuç.** Karşılaştırma yapılamadı, "doğrulanmadı". Ürünün bağlantısı veya geliştirici adı verilirse karşılaştırma tamamlanır.
- **Neyi alıyoruz, neyi almıyoruz.** Belirlenemedi.

### 2.4 Termius

- **Ne olduğu.** SSH istemcisi. Resmi fiyat sayfasına göre planlar şunlar. Starter ücretsiz (yerel vault, SSH ve SFTP, yapay zekâ destekli otomatik tamamlama, port yönlendirme). Pro ayda 10 dolar (yıllık ödemede, kişisel bulut vault, mobil ve masaüstü senkronu, oturum günlükleri, snippet otomasyonu). Team koltuk başına ayda 20 dolar (ortak vault, gerçek zamanlı işbirliği). Business koltuk başına ayda 30 dolar. Enterprise özel fiyat.
- **Lisans.** Ticari ürün. Lisans metni sayfada görülmedi, doğrulanmadı.
- **Neyi alıyoruz.** Fikirler. Profil ve gruplar, vault, port yönlendirme, kayıtlı komutlar (snippet), SFTP'nin terminalden kullanımı. Anahtar ve parola için yerel Keychain.
- **Neyi almıyoruz.** Bulut senkronu, ekip vault'ları, mobil, SFTP dosya gezgini (MVP dışı).

### 2.5 Gitfox

- **Ne olduğu.** macOS için yerel Git istemcisi (geliştirici Erik Aigner, yayıncı bytieful e.U.). Mac App Store sayfasına göre dosya, hunk ve satır bazında stage, çatışmayı farktan seçerek çözme, etkileşimli rebase (edit, reword, drop, squash, fixup, taşıma), Git Flow, birden çok kimlik, geçmiş, blame, bisect, worktree ve submodule yönetimi.
- **Fiyat ve gereksinim.** Ücretsiz indirme, uygulama içi satın alma (yıllık 49,99 dolar, aylık 7,99 dolar). macOS 15.0 ve üstü. Başka bir sayfa (Setapp) farklı bir macOS sürümü yazıyor, bu iki kaynak çelişiyor ve çözülmedi. Resmi bir geliştirici sitesi bulunamadı, kaynak Mac App Store sayfasıdır.
- **Lisans.** Ticari ürün, lisans metni görülmedi.
- **Neyi alıyoruz.** Fikirler. İyi bir Git arayüzünün neleri kapsadığı (hunk stage, çatışma seçimi, rebase). Bunları yazmak yerine lazygit'e bırakıyoruz (KK-04).
- **Neyi almıyoruz.** Kendi Git arayüzü. DCC'nin değeri agent ile branch, worktree ve değişiklik arasındaki bağdır.

### 2.6 Claudette

Bu ad birden çok projede kullanılıyor. Burada Claude Code için masaüstü uygulaması olan `utensils/claudette` ele alındı. Aynı adı taşıyan başka projeler de var (örneğin `amilner42/claudette`, bir Python paketi), bunlar incelenmedi.

- **Ne olduğu.** Tauri 2 ile yazılmış (Rust arka uç, React ve TypeScript ön yüz, SQLite) çok platformlu bir uygulama. Birden çok Claude Code agent'ını aynı anda, her biri kendi branch'inde (git worktree) çalıştırır. Plan modu, kontrol noktaları, uzak çalışma alanları, SCM entegrasyonu ve MCP sunucu denetimi var. macOS, Linux ve Windows için.
- **Lisans.** MIT (depo sayfasında).
- **Not.** README aynı anda 1 ile 3 paralel agent önerir (Claude plan koşulları nedeniyle). Sayfada kullanım ekranı görülmedi, bir arama özeti "gerçek zamanlı token kullanımı" yazdı, bu doğrulanmadı.
- **Neyi alıyoruz.** Fikirler. Agent başına worktree modeli, SQLite kaydı.
- **Neyi almıyoruz.** Ayrı bir uygulama olmak. DCC Herdr'ın içinde çalışır. Kod alma planı yok.

### 2.7 Skillshare

- **Ne olduğu.** AI kodlama ortamı için komut satırı aracı ve masaüstü uygulaması. Skills, agent'lar, kurallar, MCP bağlantıları ve hook'lar için tek kaynak tutar ve bunları araçlara senkronlar. Sayfada adı geçen hedefler Claude Code, Codex, Pi, OpenCode ve OpenClaw. Git ile sürümlenir, güvenlik denetimi ve web paneli (`skillshare ui`) var. Sayfa bir Herdr paneli de tarif ediyor.
- **Lisans ve sürüm.** MIT. Sayfadaki son sürüm v0.30.6.
- **Neyi alıyoruz.** Senkron motoru olarak kendisini. DCC, Skillshare'i çağırır (KK-06). Herdr paneli ayrıca kurulabilir.
- **Neyi almıyoruz.** Kendi senkron motorumuz.

## 3. Karşılaştırma tablosu

Yalnızca kaynaklarda görülen bilgiler yazıldı. Boş hücre "bilgi yok" demektir.

| Konu | Warp | Herdr | Termius | Gitfox | Claudette | Skillshare |
| --- | --- | --- | --- | --- | --- | --- |
| Terminal | Evet | Evet | SSH istemcisi | Hayır | Hayır (Claude Code oturumları) | Hayır |
| Agent durum takibi | Agent odaklı (ayrıntı kaynakta yok) | Evet (OSC 7501, spike) | Hayır | Hayır | Oturum yönetimi var | Hayır |
| Agent ve worktree bağı | Bilgi yok | Spike'ta DCC ile kuruldu | Hayır | Worktree yönetimi var | Evet (agent başına worktree) | Hayır |
| SSH yönetimi | Bilgi yok | Uzak makine bağlantısı var (doğrulanmadı) | Evet | Hayır | Uzak çalışma alanı var | Hayır |
| Git arayüzü | Bilgi yok | Hayır | Hayır | Evet | SCM entegrasyonu var | Hayır |
| AI konfigürasyon | Bilgi yok | Hayır | Hayır | Hayır | MCP denetimi var | Evet |
| Lisans | AGPL v3 ve MIT | Doğrulanmadı | Ticari | Ticari | MIT | MIT |

## 4. Hibrit kararının gerekçesi

Kanıtlar ve eksikler ayrı yazıldı.

**Kanıtlı.**

1. Spike Adım 1 ile 6'da şunlar Herdr'a dokunmadan çalıştı. Mod ile dış program çağırma, Keychain gidiş dönüşü, SQLite kalıcılığı, 41 sn'lik detached iş, workspace geri yükleme, gerçek Claude Code durumu ve salt okunur agent paneli.
2. Tek başına Herdr modları yetmiyor. Mod kodunda stdin yok, `exec` 30 sn'de öldürülüyor, modlar sandbox'suz. Bu yüzden gizli bilgi, SQLite, git ve uzun işler dcc'ye taşındı. Hibritin ikinci ayağı spike ile gerekçelendi.
3. Çekirdek değişikliği gerektiren işler az ve MVP dışı. Pane boyutlarının kalıcılığı ve komut blokları. Üçüncü aday olay akışı, ama bu doğrulanmadı.
4. Agent durumu Herdr'da hazır geliyor (agent'ın kendi raporu). Bunu başka bir terminalde yeniden kurmak ek iş olurdu.

**Eksik veya doğrulanmadı.**

1. Warp'ın kod tabanı büyüklüğü, derleme ve bakım yükü ölçülmedi. Warp ile aynı yeteneklerin zorluğu karşılaştırılmadı.
2. Herdr masaüstü ve core lisansı doğrulanmadı. Bu çalışmada Herdr'ı fork'lamak veya dağıtmak planlanmıyor, ama dağıtım kararı (KK-13) bu bilgiyi gerektirecek.
3. Herdr 0.1.x için API kararlılığı taahhüdü görülmedi. Bu risk, sürüm sabitleme ve sözleşme testiyle yönetilir (KK-09).

**Neden fork değil.**

- Warp kaynağı AGPL v3. Kod almak veya fork'lamak, dağıtımda aynı lisans yükümlülüğünü getirir. Dağıtım şimdilik hedef değil ama kapıyı kapatmamak istenir (KK-13).
- DCC'nin değeri bir terminal motoru yazmak değil, agent, branch ve worktree bağı gibi orkestrasyon katmanıdır. Bu katman spike'ta Herdr üzerinde kuruldu.
- Fork, güncellemeleri izleme yükünü bize bırakır. Hibritte Herdr güncellemesi yalnızca layout kopyasının yeniden üretilmesi ve sözleşme testi demektir (`docs/03-architecture.md`).

## 5. Kaynaklar (erişim 2026-11-12)

- Termius fiyatlar sayfası, [termius.com/pricing](https://termius.com/pricing). Planlar ve özellik listesi.
- Gitfox, [Mac App Store sayfası](https://apps.apple.com/us/app/gitfox/id1475511261). Özellikler, fiyat ve macOS gereksinimi. Çelişen sürüm bilgisi için [Setapp sayfası](https://setapp.com/apps/Gitfox) arama sonucu olarak görüldü, açılıp doğrulanmadı.
- Warp, [github.com/warpdotdev/warp](https://github.com/warpdotdev/warp). Lisans bölümü (AGPL v3, MIT arayüz kütüphaneleri) ve katkı süreci. Oz bilgisi ikincil haber kaynaklarından, doğrulanmadı.
- Herdr TUI deposu, [github.com/ogulcancelik/herdr](https://github.com/ogulcancelik/herdr). Apache 2.0 notu ve tanım. Bir arama özeti aynı depo için AGPL-3.0-or-later yazdı, çelişki çözülmedi.
- Claudette, [github.com/utensils/claudette](https://github.com/utensils/claudette). MIT, Tauri 2, worktree başına agent.
- Skillshare, [github.com/runkids/skillshare](https://github.com/runkids/skillshare). MIT, v0.30.6, Herdr paneli.
- TermSplit. Resmi kaynak bulunamadı (iki arama).
- Spike bulguları, `~/dev/dcc-spike/SPIKE-REPORT.md` ve `SPIKE-SUMMARY.md`.
