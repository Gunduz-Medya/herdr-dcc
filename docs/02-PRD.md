# DCC ürün gereksinimleri (PRD)

Sürüm 0.1, taslak. Dayanaklar `docs/01-capability-inventory.md` ve spike raporları (`~/dev/dcc-spike`). Spike'ta denenmeyen her şey "doğrulanmadı" diye işaretlidir.

## 1. Vizyon

Developer Command Center (DCC), geliştiricinin terminalde yaptığı işleri tek pencerede toplar. Birden çok proje ve birden çok AI agent aynı anda çalışır. DCC her agent'ın ne yaptığını, hangi branch ve worktree'de olduğunu, kaç dosya değiştirdiğini ve ne zaman sana ihtiyaç duyduğunu gösterir. Terminal, SSH, Git ve AI ayarları aynı çatı altında kalır, ama her şeyi sıfırdan yazmak yerine iyi çalışan araçlar (Herdr, lazygit, neovim, Skillshare) birbirine bağlanır.

## 2. Kullanıcı

Tek kişi, geliştirici (Barış Gündüz). macOS Apple Silicon kullanıyor, Claude Code ile günlük çalışıyor, birden çok repo ve worktree arasında geçiyor, uzak makinelere bağlanıyor. Takım, hesap yönetimi veya dağıtım şimdilik yok.

## 3. Problem

- Birden çok agent çalışırken hangisinin izin beklediğini, hangisinin bittiğini veya takıldığını görmek için pane pane gezmek gerekiyor.
- Kapanan bir agent oturumu hakkında (hangi branch, kaç değişiklik, nasıl bitti) hiçbir kayıt kalmıyor. Spike'ta Herdr'ın çıkan oturumu panelden kaybettiği görüldü (risk 20).
- Workspace'i (klasörler, sekmeler, başlangıç komutları) yeniden kurmak elle yapılıyor. Herdr kendi başına yalnızca boş shell'leri geri getiriyor.
- SSH profilleri, parolalar, kayıtlı komutlar ve AI ayarları (skills, rules, MCP) farklı yerlerde dağınık.
- Kullanım ve kota bilgisi yalnızca çalışan oturumun içinde görünüyor.

## 4. Hedefler ve hedef dışı

Hedefler

- Çoklu agent durumunu tek panelde, çekirdek durumuna dayanarak ve gecikmesi birkaç saniyeyi geçmeden göstermek.
- Agent ile branch, worktree ve değişiklik arasında güvenilir bir bağ kurmak.
- Kapanan oturumları SQLite'ta tutup Completed, Stopped ve Failed ayrımını göstermek.
- Workspace'i kaydetmek ve geri yüklemek (yaklaşık).
- SSH profilleri, gizli bilgi ve kayıtlı komutlar için güvenli bir yönetici.
- Lazygit ve neovim ile kod sekmesi.
- AI konfigürasyonunu okumak (envanter) ve Skillshare'i çağırmak.
- Kullanım ve kota bilgisini yalnızca resmi kaynaktan, yoksa "Unavailable" diye göstermek.

Hedef dışı (MVP için)

- Herdr'ı fork'lamak veya çekirdeğini değiştirmek. Warp'ı fork'lamak. Warp kaynağından kod almak.
- Komut blokları (çıkış kodu işareti, son çıktıyı kopyala, çıktıyı katlama). Sonraki sürüme not edildi.
- Kendi Git arayüzü (diff, stage, commit, rebase, çatışma). Bunun için lazygit kullanılır.
- Kendi AI konfigürasyon senkron motoru.
- SFTP dosya gezgini ve sürükle bırak.
- Üçüncü taraf mod ekosistemi, kod imzalama, güncelleme kanalı, lisans kararı.
- Windows (ayrı ve son faz).
- `~/.claude` altına herhangi bir yazma.

## 5. Modül başına kapsam

### 5.1 Terminal ve workspace

- Herdr'ın sekme, split ve çoklu terminal özellikleri olduğu gibi kullanılır.
- DCC kendi layout kopyasını taşır (gönderilmiş layout'lar salt okunur, risk 1) ve Herdr güncellemesinde yeniden kurar.
- Workspace kaydet ve geri yükle. Klasör, sekme adları, pane sırası, başlangıç komutları. Split yönü ve boyutu okunamadığı için yaklaşık geri yükleme (KK-05).
- Space kapanınca terminalleri bilerek kapatır veya park eder (space.close terminalleri kapatmıyor).

### 5.2 Agent yönetimi

- Salt okunur çoklu agent paneli. Satır başına durum, oturum kimliği, branch, worktree etiketi, değişen dosya sayısı (staged, unstaged, yeni), son olay.
- Durum kaynağı Herdr çekirdek durumu. Hook'lar yalnızca kimlik ve ek bilgi içindir (KK-09).
- DCC'nin kendi başlattığı agent'lar `claude --settings <geçici dosya> --session-id <uuid>` ile açılır, temiz ve açık bir ortamla. Hook ve statusline bu ayar dosyasından gelir (KK-02).
- Dışarıdan başlatılan agent'larda yalnızca Herdr durumu ve klasör eşlemesi gösterilir.
- Oturum kaydı SQLite'ta. Kapanan oturum son durumuyla (Completed, Stopped, Failed) listede kalır (KK-11).
- Güven sorusu bekleyen agent "Unknown, güven sorusu olabilir" diye görünür. DCC onay vermez.
- Agent başına ayrı worktree önerilir ve DCC bunu otomatikleştirir (risk 22).

### 5.3 SSH yöneticisi

- Profil ve grup (ad, host, kullanıcı, port, etiket) SQLite'ta.
- Anahtar ve parola Keychain'de. Giriş gizli bir terminal panelinden alınır.
- Bağlanma, hc machine veya ssh ile pane içinde. Port yönlendirme dcc ve job deseniyle.
- Kayıtlı komutlar (snippet). Çalıştırma hc send ile.
- SFTP yalnızca terminalden (sftp, rsync). Dosya gezgini kapsam dışı.

### 5.4 Git ve kod sekmesi

- Branch, worktree ve değişiklik bilgisi dcc'den gelir (salt okunur).
- Diff, stage, commit, rebase ve çatışma için lazygit pane veya popup olarak çalışır (KK-04).
- "Kod sekmesi" şablonu. Solda neovim, sağda lazygit, altta shell.
- Odaktaki panelin klasöründe lazygit popup'ı.

### 5.5 AI konfigürasyon

- MVP'de envanter (okuma ve listeleme). Skills, rules, MCP, hook ve plugin dosyaları ve hangi agent'a ait oldukları.
- Senkron için Skillshare çağrılır, kendi motor yazılmaz (KK-06).
- Yapılandırma dosyasına yazan her özellik önce yedek, diff önizleme, dry-run ve silme için ayrı onay ister.

### 5.6 Kullanım ve kota

- Bağlam kullanımı (oturum içi) ve hesap kotası ayrı bölümlerde gösterilir.
- Kaynak yalnızca DCC'nin başlattığı agent'ların statusline verisidir. Kaynak yoksa "Unavailable" yazar. Hiçbir rakam tahmin edilmez veya hesaplanmaz (KK-10).
- Geçmiş grafiği yalnızca DCC çalışırken kaydedilen örneklerden oluşur.

### 5.7 Altyapı

- Gizli bilgi, SQLite, uzun işler (job deseni), OSC 7501 sözleşme testi, çekirdek yaşam döngüsü, test izolasyonu. Ayrıntı `docs/03-architecture.md` içinde.

## 6. MVP sınırı

MVP, macOS Apple Silicon üzerinde yol haritasının Faz 1 ile Faz 6 arasıdır (çekirdek, agent paneli ve oturum kaydı, SSH, Git ve kod sekmesi, AI konfigürasyon, kullanım ve kota). Faz 0 yalnızca doğrulama testleridir. Faz 7 (Windows) MVP dışıdır.

MVP içinde olmayanlar yukarıdaki "Hedef dışı" listesidir. Her fazın sonunda kullanıcıya gösterilebilir somut bir sonuç vardır (`docs/05-roadmap.md`).

## 7. Kabul ölçütleri

Her ölçüt sınanabilir ve ilgili faz sonunda tek tek kontrol edilir.

| Modül | Ölçüt |
| --- | --- |
| Çekirdek | Workspace kaydedilip core yeniden başlatıldıktan sonra geri yüklenince aynı sekmeler, klasörler ve başlangıç komutları kuruluyor |
| Çekirdek | `dcc` komutları 2 sn altında dönüyor, 30 sn'yi aşabilecek iş job desenini kullanıyor |
| Agent paneli | İki agent aynı anda iki satır gösteriyor (farklı branch ve worktree etiketi), durum değişikliği panele 3 sn içinde (medyan) yansıyor |
| Agent paneli | Bir agent çıkınca satır oturum kaydından geliyor ve son durumu (Completed, Stopped veya Failed) gösteriyor |
| Agent paneli | `--session-id` ile başlatılan agent'ın kimliği argv'den, hook'tan ve kayıttan aynı çıkıyor |
| SSH | Profil eklenir, parola Keychain'e girilir (argv ve log temiz), bağlantı açılır, port yönlendirme job olarak çalışır |
| Git ve kod sekmesi | Kod sekmesi şablonu kurulur ve geri yüklenir, lazygit popup'ı odaktaki panelin klasöründe açılır |
| AI konfigürasyon | Envanter agent başına dosya yollarını ve türlerini listeler, yazan her eylem yedek ve dry-run önizleme gösterir |
| Kullanım ve kota | Bağlam ve kota ayrı başlıklarda, kaynak yoksa "Unavailable", kodda hiçbir tahmin yok |
| Güvenlik | Sır argv'de, logda, SQLite'ta ve test çıktısında bulunmuyor (tarama testi), `secret get` mod yüzeyinde yok |
| Genel | Her fazda `cargo test` ve `cargo clippy` temiz, mod senaryo testleri izole ortamda geçiyor |

## 8. Güvenlik ve gizlilik

- Gizli bilgi macOS Keychain'de tutulur (keyring crate). `get` komutu mod yüzeyine asla açılmaz, yalnızca `exists` ve sırrı kendi içinde kullanan komutlar açılır (risk 5). Sır argv'ye ve loglara girmez. Giriş gizli bir terminal panelinden alınır, çünkü exec'te stdin yok (risk 4).
- Modlar sandbox'suz çalışır (spike Adım 2). Yalnızca kendi modlarımız kurulur, üçüncü taraf mod politikası yoktur.
- Hook, statusline ve durum `msg` alanı konuşma metni ve komut içerebilir. Yalnızca izinli alanlar (kimlik, yol, durum adı, sayı, zaman damgası) yazılır, `msg` hiç yazılmaz (risk 16, 17).
- DCC `~/.claude` dosyalarına yazmaz ve gizli bilgi dosyalarını okumaz. Kendi başlattığı agent'a geçici ayar dosyası verir.
- Test ve geliştirme izole ortamda (ayrı `HERDR_APP_HOME`, `HC_SESSION`) yapılır. Claude Code Herdr izolasyonuyla izole olmaz (risk 19).
- Yapılandırma dosyasına yazan her özellik yedek, diff önizleme, dry-run ve silme için ayrı onay ister.
- Agent'a verilen onayları (güven, izin) DCC kendisi vermez.

## 9. Platform

MVP yalnızca macOS Apple Silicon. Kod platformdan bağımsız yazılır (yol birleştirme, süreç ve dosya işlemleri, Keychain dışında macOS'a özgü çağrı yok). Windows ayrı ve son fazdır. Windows'ta bir şey çalıştırılmadığı sürece "çalışıyor" iddiası yazılmaz. Spike'tan gelen Windows riskleri `docs/03-architecture.md` ve `docs/05-roadmap.md` içinde listelenmiştir.

## 10. Karar kayıtları

| No | Karar | Dayanak | Durum |
| --- | --- | --- | --- |
| KK-01 | Hibrit yaklaşım. Herdr masaüstü uygulaması ve core üzerinde modlar (JavaScript) ve küçük bir Rust yardımcısı (dcc). Warp fork edilmez, yalnızca referans, Warp kaynağından kod alınmaz | Spike Adım 1 ile 6, `docs/04-comparative-analysis.md` | Verildi |
| KK-02 | Agent verisi (kimlik, model, bağlam, kota) yalnızca DCC'nin başlattığı agent'larda `claude --settings` ve `--session-id` ile alınır. DCC `~/.claude` ayarlarına yazmaz. Dışarıdan başlatılanlarda yalnızca Herdr durumu ve klasör eşlemesi | Risk 8, 12. `--settings` doğrulanmadı (Faz 0 test 1) | Verildi |
| KK-03 | MVP yalnızca macOS Apple Silicon, Windows ayrı ve son faz | Spike yalnızca macOS | Verildi |
| KK-04 | Git için lazygit pane veya popup. Kendi Git arayüzü MVP dışı. DCC'nin değeri agent, branch, worktree ve değişiklik bağı | Envanter, Gitfox karşılaştırması | Verildi |
| KK-05 | Pane boyutları kalıcı olmaz (API yok), MVP'de yaklaşık geri yükleme kabul. Seçenek Herdr'ı fork'lamaktı | Spike Adım 4. Herdr'ın kendi boyut korunumu doğrulanmadı | Varsayılan kabul |
| KK-06 | AI konfigürasyon senkronu için Skillshare çağrılır, kendi motor yazılmaz. MVP'de envanter (okuma) | Skillshare MIT, Herdr paneli var (kaynak taraması) | Verildi |
| KK-07 | Komut blokları MVP dışı. Çıkış kodu işareti ve son çıktıyı kopyala sonraki sürüme not | Core Change gerekiyor | Verildi |
| KK-08 | Gizli bilgi Keychain'de, `get` mod yüzeyine açılmaz, sır argv ve loglara girmez | Spike Adım 2, 3a | Verildi |
| KK-09 | Agent durumu için Herdr çekirdek durumu esas, hook'lar yalnızca kimlik ve ek bilgi. OSC 7501 belgesiz olduğundan Herdr sürümü sabitlenir ve her sürümde sözleşme testi çalışır | Spike Adım 5, 6, risk 2, 10, 11 | Verildi |
| KK-10 | Kullanım ve kota rakamı tahmin edilmez, resmi kaynak yoksa "Unavailable". Bağlam ve kota ayrı gösterilir | Spike Adım 5 | Verildi |
| KK-11 | Kapanan agent oturumları için DCC'nin kendi SQLite kaydı zorunlu | Risk 20 | Verildi |
| KK-12 | 30 sn'yi aşabilecek her iş job desenini kullanır (start, status, clean, sweep) | Spike Adım 3, 4b | Verildi |
| KK-13 | Dağıtım şimdilik yalnızca kendi kullanım. Lisans, imzalama ve güncelleme kanalı ertelenir, mimari dağıtıma açık kalır | Risk 6 | Verildi |
| KK-14 | Agent başına ayrı worktree önerilir ve otomatikleştirilir. Aynı klasörde iki agent uyarı üretir | Risk 22 | Varsayılan kabul |
| KK-15 | Kod sekmesi şablonu (neovim, lazygit, shell) MVP içinde. Kurulum dış araçlar için kullanıcıya bırakılır, DCC bir şey kurmaz | Envanter | Verildi |
| KK-16 | Test kuralı. Her faz branch'te kalır, merge'i kullanıcı yapar, main'e doğrudan yazılmaz | Çalışma kuralı | Verildi |

## 11. Açık sorular

1. Herdr masaüstü uygulaması ve core hangi lisansla dağıtılıyor. Araştırmada yalnızca bir TUI deposu bulundu ve o sayfa Apache 2.0 diyor, ama o sayfa masaüstü uygulamasını veya core'u anmıyor. Doğrulanmadı.
2. Herdr 0.1.x için API kararlılığı taahhüdü var mı. Belge taramasında görülmedi.
3. `claude --settings` ile hook ve statusline enjeksiyonu çalışıyor mu. Faz 0 test 1.
4. Codex, Gemini CLI, Cursor ve Grok OSC 7501 yayınlıyor mu. Denenmedi.
5. `/clear` veya resume sonrası `--session-id` ile verilen kimlik ne oluyor (risk 21). Denenmedi.
6. Mod API'sinde olay akışı (`events.subscribe`) var mı. d.ts taraması tam değil.
7. SSH port yönlendirme ve uzak makine bağlantısı Herdr'da nasıl davranıyor. Faz 0 test 5.
8. Keychain, imzasız ikili güncellenince onay penceresi çıkarıyor mu (risk 6). Denenmedi.
9. Windows'ta hangi yetenekler çalışıyor. Hiçbiri sınanmadı.
10. Lazygit ve neovim Herdr pane'inde ve popup olarak nasıl çalışıyor. Faz 0 test 2 ve 3.
