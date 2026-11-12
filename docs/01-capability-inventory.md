# DCC yetenek envanteri

Bu doküman `docs/inputs/DCC Yetenek Envanteri.md` dosyasının spike bulgularına göre düzeltilmiş halidir. Orijinal dosyaya dokunulmadı. Değişen satırlar "(düzeltildi)" ile işaretli, gerekçeler sonda "Düzeltmeler" başlığında. Kaynaklar `~/dev/dcc-spike/SPIKE-SUMMARY.md` ve `SPIKE-REPORT.md` (Adım 1 ile 6).

Ortam. macOS Apple Silicon, Herdr uygulaması (herdr-app) 0.1.15, herdr-core 0.1.20, Claude Code 2.1.322. Herdr 0.1.x sürüm çizgisinde. API kararlılığı taahhüdü belgelerde görülmedi, bu doğrulanmadı ve ayrı bir risktir (risk 2).

## Okuma kılavuzu

| Terim | Anlamı |
| --- | --- |
| Native | Herdr'da hazır, bizim yazacağımız bir şey yok |
| Mod | Herdr'ın JavaScript modu ile yapılır (panel, komut, overlay) |
| dcc | Bizim Rust yardımcımız yapar (Keychain, SQLite, git, uzun işler) |
| Dış araç | Hazır program kullanılır (lazygit, ssh, neovim) |
| Core Change | Herdr kaynak kodunu değiştirmek gerekir |
| Kolay | Bilinen bir API ile doğrudan yapılır |
| Orta | Birkaç parçanın birleşmesi veya bilinen bir sınırın etrafından çalışmak gerekir |
| Zor | Belirsiz veya büyük bir iş |
| Spike Adım N | Çalıştırılıp gözlendi |
| Kaynak taraması | Belge ve kod okunarak çıkarıldı, çalıştırılmadı |
| Denenmedi | Hiçbir kanıt yok, zorluk tahmindir |

Herdr çekirdeğini değiştirmek iki iş için gerekiyor. Pane boyutlarının kalıcılığı ve komut blokları. İkisi de MVP dışı. Üçüncü bir aday olay akışıdır. Çekirdekte `events.subscribe` var, mod API'sinde görülmedi (d.ts taraması tam değil, doğrulanmadı). Bu olmazsa DCC polling ile çalışır (düzeltildi).

## Terminal ve workspace

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Sekmeler, dikey ve yatay split, tek ekranda çoklu terminal | Native | Kolay | Spike Adım 4 | Sekmeler ve klasörler geri yüklemede doğru geldi |
| Terminallerin core'da yaşaması (düzeltildi) | Native | Kolay | Spike Adım 4 | Core yeniden başlayınca adres ve son klasör korunuyor ama shell yeni bir süreç, içinde çalışan program kaybolur. Pencere kapanınca yaşaması kaynak taramasıdır |
| Proje workspace'i (klasör, sekmeler, başlangıç komutları) kaydet ve geri yükle | dcc + Mod | Orta | Spike Adım 4 | Gönderilmiş layout'lar salt okunur, DCC kendi layout kopyasını taşır (risk 1) |
| Workspace kapanınca terminalleri kapatma veya park etme (düzeltildi) | dcc + Mod | Orta | Spike Adım 4, 5, 6 | space.close terminalleri kapatmıyor, hc kill gerekti. Adım 4'te 4 terminalden 2'si yaşadı ve fazladan bir space doğdu, hangi adımın yarattığı doğrulanmadı |
| Pane boyutlarının (split oranı) kalıcı olması (düzeltildi) | Core Change | Zor | Spike Adım 4 | Hiçbir API'den okunamıyor. Herdr'ın kendi yeniden başlatmada boyutu koruyup korumadığı doğrulanmadı. MVP'de yaklaşık geri yükleme kabul edilir |
| Neovim ve lazygit düzeni ("kod sekmesi" şablonu) | Native + Mod | Kolay | Kaynak taraması | Terminal paneli olarak çalışır, şablon layout olarak eklenir. Herdr'da denenmedi (Faz 0 test 3) |
| Lazygit popup (odaktaki panelin klasöründe) | Mod | Orta | Kaynak taraması | TUI'deki HERDR\_ACTIVE\_PANE\_CWD benzeri değişken yok, mod focused().cwd kullanır. Denenmedi (Faz 0 test 2) |
| Pane bölerken klasör verme | Mod | Kolay | Spike Adım 4 | pane.split'te klasör ve odaksız bölme yok, terminal.open {folder, place} ile aşılır. Üç ve daha fazla pane ile denenmedi |
| Tuş atamaları ve komut tetikleme | Native | Kolay | Kaynak taraması | keys.json, mod komutlarını bağlayabilir |
| Komut blokları, çıkış kodu işareti | Core Change | Orta | Denenmedi | Kabuk entegrasyonu ve komut sınırı takibi gerekir. MVP dışı |
| Komut blokları, son çıktıyı kopyala | Core Change | Orta | Denenmedi | Aynı altyapı. MVP dışı |
| Komut blokları, çıktıyı katlama | Core Change | Zor | Denenmedi | Çizim ve girdi editörü değişir. Değer düşük bulundu, ertelendi |

## SSH yöneticisi

Profil, Keychain ve uzun iş altyapısı spike'ta çalıştı. Port yönlendirme ve SFTP hiç denenmedi, buradaki zorluklar tahmindir.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Uzak makineye bağlanma (uzakta terminal açma) | Native | Orta | Kaynak taraması | herdr-core makineleri SSH üzerinden bağlayabiliyor (hc machine). Uzak tarafta ne gerektiği doğrulanmadı (Faz 0 test 5) |
| SSH profilleri ve grupları (ad, host, kullanıcı, port, etiket) | dcc + Mod | Kolay | Spike Adım 3, 4 | Kayıtlar SQLite'ta, panel modda. SQLite yazma ve okuma çalıştı |
| Anahtar ve parola saklama (macOS Keychain) | dcc | Orta | Spike Adım 2, 3a | Gizli bilgi argv'ye ve mod yüzeyine çıkmaz. İmzasız ikili sorunu denenmedi (risk 6) |
| Anahtar üretme ve ssh-agent ile kullanma | Dış araç | Orta | Denenmedi | ssh-keygen ve ssh-agent'ı dcc sarmalar |
| Port yönlendirme (local, remote, dynamic) | dcc | Orta | Denenmedi | ssh süreci olarak çalışır, uzun işler için job deseni uygulanır (Spike Adım 4b) |
| SFTP, terminalden (sftp, rsync) | Dış araç | Kolay | Denenmedi | Pane içinde çalışır |
| SFTP, dosya gezgini ve sürükle bırak | Mod | Zor | Denenmedi | Mod arayüzü, aktarım kuyruğu ve ilerleme gerekir. MVP dışı |
| Kayıtlı komutlar (snippet) | Mod + dcc | Kolay | Spike Adım 3, 4 | Komutlar SQLite'ta, çalıştırma hc send ile. hc send bloklu programa --force olmadan yazmaz |
| Parola ve passphrase girişi | Mod + dcc | Kolay | Spike Adım 2, 3a | exec'te stdin yok, giriş gizli bir terminal panelinden alınır |

## Git yöneticisi

DCC'nin kendi değeri agent ile branch, worktree ve değişiklik arasındaki bağ. Diff, stage ve rebase için lazygit'i panelde çalıştırmak, sıfırdan Git arayüzü yazmaktan çok daha hızlı.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Branch, worktree ve değişen dosya sayısı | dcc | Kolay | Spike Adım 5, 6 | Klasör bazında güvenilir (main, ayrı worktree, detached, alt klasör, silinmiş worktree). Okuma salt okunur ve `--no-optional-locks` ile yapılır |
| Agent ile branch, worktree ve değişiklik bağı | dcc + Mod | Orta | Spike Adım 5, 6 | Aynı klasördeki iki agent ayrılamaz (risk 22) |
| Agent başına ayrı worktree otomasyonu | dcc | Orta | Denenmedi | Risk 22'nin önerilen çözümü. git worktree add çağrısını dcc yapar |
| Worktree oluşturma, silme, listeleme | dcc | Kolay | Spike Adım 5 | Spike'ta elle kuruldu, dcc ile otomatik denenmedi |
| Diff görüntüleme, stage ve unstage, commit | Dış araç (lazygit) | Kolay | Kaynak taraması | Herdr'da denenmedi |
| Branch, stash, rebase, çatışma çözme | Dış araç (lazygit) | Kolay | Kaynak taraması | Lazygit kapsıyor, bizim kodumuz gerekmez |
| Lazygit'i odaktaki panelin repo'sunda açma | Mod | Orta | Kaynak taraması | focused().cwd ile. Yeni pane için terminal.open {folder, place} kullanılır. Denenmedi |
| Kendi Git arayüzü (Gitfox benzeri diff, stage, rebase) | Mod | Zor | Denenmedi | gpui-kit ile çizilir. Büyük iş, MVP dışı |

## Agent yönetimi

Claude Code durumu ve kimliği spike'ta güvenilir çıktı. Kapanan oturumlar için DCC'nin kendi kaydı şart, diğer agent'lar ise hiç denenmedi.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Durum takibi (Claude Code) (düzeltildi) | Native | Kolay | Spike Adım 5, 6 | Herdr durumu Claude'un OSC 7501 raporundan alıyor, kanal belgesiz (risk 2). Esas alınacak kaynak çekirdek durumu. `agents()` görülünce `done` değerini `idle` yapıyor (risk 10). Hook'lar durum için kullanılmaz |
| Durum takibi (Codex, Gemini CLI, Cursor, Grok) | Native veya dcc | Orta | Denenmedi | OSC 7501 yayınlayıp yayınlamadıkları bilinmiyor. Yayınlamıyorlarsa dcc adapter'ı gerekir |
| Ortak durum modeli (Running, Waiting, Completed, Failed, Stopped, Unknown) | dcc | Kolay | Spike Adım 6 | Eşleme kuralı yazıldı ve 9 testle denendi. Failed ve Stopped canlı gözlenmedi |
| Oturum kimliği | dcc | Kolay | Spike Adım 5, 6 | DCC agent'ı --session-id ile başlatırsa kimlik Herdr argv'sinde ve hook'ta aynı çıktı. Dışarıdan başlatılanlarda yok. `/clear` sonrası davranış denenmedi (risk 21) |
| Agent başlatma (worktree, ayar ve kimlikle) | dcc + Mod | Orta | Denenmedi | hc run veya terminal.open kullanılır. --settings ile hook enjeksiyonu test edilmedi (Faz 0 test 1). Ortam temiz ve açık verilir (risk 15) |
| Oturum kaydı (kapanan oturum, son durum, branch, değişiklik) | dcc | Orta | Spike Adım 6 (boşluk) | panes() çıkan oturumu kaybediyor. Completed, Stopped ve Failed için DCC'nin kendi SQLite kaydı şart (risk 20) |
| Güven sorusu bekleyen agent | dcc + Mod | Kolay | Spike Adım 5 | Herdr durum göstermiyor (yaklaşık 132 sn). DCC "Unknown, güven sorusu olabilir" gösterir, kendisi onaylamaz. Panelde canlı görülmedi |
| Bildirim (izin bekliyor, bitti) | dcc + Mod | Orta | Spike Adım 6 | Polling ile kısa durumlar kaçıyor (19 geçişin 3'ü). Notification hook'u geç ve isteğe bağlı (risk 11) |
| Agent'a prompt gönderme ve çıktı okuma | Mod | Kolay | Kaynak taraması | hc send ve hc read. Çıktı konuşma metni içerebilir, loglanmaz |
| Çoklu agent paneli (durum, kimlik, branch, worktree, değişen dosya) (düzeltildi) | Mod | Kolay | Spike Adım 6 | Salt okunur panel çalıştı, medyan gecikme 1,1 sn. Çıkan oturum panelden kayboluyor, bu kısım oturum kaydına bağlı (Orta) |

## AI konfigürasyon yöneticisi

Bu bölüm spike'ta hiç denenmedi. Zorluklar Skillshare kaynak taramasına ve agent'ların yapılandırma düzenine dayanan tahmindir.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Skills, rules, MCP, hook ve plugin envanteri (okuma ve listeleme) | dcc | Orta | Denenmedi | Her agent'ın yapılandırma yolu ve biçimi ayrı. Salt okunur başlanır |
| Agent'lar arası senkron | Dış araç (Skillshare) | Orta | Kaynak taraması | Skillshare MIT lisanslı bir CLI, Herdr paneli var. Kendi senkron motoru yazılmaz, Skillshare çağrılır (karar) |
| Güvenli yazma (yedek, diff önizleme, dry-run, silme için ayrı onay) | dcc | Orta | Denenmedi | Zorunlu kural. Yapılandırma dosyalarına yazan her özellik bunu önce sağlar |
| Claude global ayarlarına (\~/.claude) yazma | Kapsam dışı | Yok | Karar | DCC hiçbir koşulda yazmaz. Kendi başlattığı agent'a --settings verir |
| Codex, Cursor, Gemini CLI için hook ve plugin yönetimi | dcc | Zor | Denenmedi | Her agent'ın ayar şeması farklı ve sürümle değişebilir |

## Kullanım ve kota panosu

Resmi kaynak olan tek yer çalışan oturumun statusline verisi. Bunun dışında hiçbir rakam tahmin edilmez, kaynak yoksa panoda Unavailable yazar.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Oturum bağlam kullanımı (context penceresi) (düzeltildi) | dcc | Orta | Spike Adım 5 | Yalnızca çalışan oturumun statusline JSON'unda (context\_window alanları). Alan ilk API çağrısına kadar null olabilir. Yalnızca DCC'nin başlattığı agent'larda gelir |
| Hesap kotası (5 saatlik ve 7 günlük limit) (düzeltildi) | dcc | Orta | Spike Adım 5 | rate\_limits alanı 12 çağrının 11'inde vardı, ilk API yanıtına kadar yok. Bağımsız ve programatik bir kota sorgusu bulunamadı. Abonelik dışı hesaplarda varlığı doğrulanmadı |
| Diğer agent'lar için kullanım ve kota | dcc | Belirsiz | Denenmedi | Resmi kaynak araştırılmadı. Bulunana kadar Unavailable |
| Geçmiş kullanım grafiği | dcc | Kolay | Denenmedi | Yalnızca DCC çalışırken kaydedilen örneklerden oluşur, geriye dönük veri yok |
| Tahmin ve hesaplama | Yok | Yok | Karar | Resmi kaynak yoksa Unavailable yazılır. Bağlam kullanımı ile hesap kotası ayrı gösterilir |

## Altyapı ve platform

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Dış program çağırma | Mod | Kolay | Spike Adım 2, 3, 4a, 6 | exec çağrıları 13 ile 90 ms. 30 sn sınırı, stdin ve env yok |
| Uzun işler (detached job) | dcc | Orta | Spike Adım 3, 4b | 41 sn'lik iş sırasında pencere donmadı, başlatan süreç öldürülünce iş yaşadı. Yalnızca macOS, Windows kolu derlenmedi |
| Gizli bilgi (Keychain) | dcc | Orta | Spike Adım 2, 3a | get komutu mod yüzeyine açılmaz (risk 5) |
| Yerel veri (SQLite) | dcc | Kolay | Spike Adım 3, 4 | WAL modunda terminal ve iş aynı anda yazdı. Yük testi yok |
| Üçüncü taraf mod güvenliği | Politika | Orta | Kaynak taraması | Mod kodu sandbox'suz çalışıyor. Şimdilik yalnızca kendi modlarımız |
| OSC 7501 sözleşmesi için sürüm sabitleme ve test | dcc | Orta | Spike Adım 5 | Biçim belgesiz (risk 2). Her Herdr sürümünde kısa bir sözleşme testi çalışır |
| DCC modunun kurulumu ve güncellenmesi | dcc | Kolay | Spike Adım 1, 6 | Masaüstünde plugin install yok. Mod klasörü `<app home>/mods` altına kopyalanır, herdr-app check ile doğrulanır |
| Herdr güncellemesinde layout kopyasının yeniden kurulması | dcc | Orta | Spike Adım 1, 4 | Gönderilmiş layout'lar salt okunur (risk 1) |
| Çekirdek yaşam döngüsü (başlatma, durdurma, doğrulama) (eklendi) | dcc | Kolay | Spike Adım 4, 5, 6 | hc server status çekirdek yoksa yenisini başlatıyor, canlılık sınaması için kullanılmaz (risk 14). Kapanış PID ile doğrulanır |
| Agent ortamının temizlenmesi (eklendi) | dcc | Kolay | Spike Adım 5 | Terminaller çekirdeği başlatan sürecin ortamını miras alıyor (risk 15). Agent açık ve temiz ortamla başlatılır |
| Günlük ve log alan beyaz listesi (eklendi) | dcc | Kolay | Spike Adım 5 | Hook, statusline ve durum msg alanı konuşma metni taşıyabilir. Yalnızca izinli alanlar yazılır (risk 16, 17) |
| Test izolasyonu ve ayrı profil (eklendi) | dcc | Orta | Spike Adım 5 | Herdr izolasyonu ~/.claude ve ~/.claude.json dosyalarını izole etmiyor (risk 19) |
| Windows | Hepsi | Belirsiz | Denenmedi | MVP macOS. Windows ayrı ve son aşama, her yetenek orada yeniden doğrulanır |
| Kod imzalama, güncelleme, dağıtım | Ertelendi | Orta | Karar | Şimdilik yalnızca kendi kullanımın. Keychain imza sorunu (risk 6) dağıtım kararına kalır |

## Verilen kararlar

- Agent verisi, yalnızca DCC'nin başlattığı agent'larda `--settings` ve `--session-id` ile alınır. `~/.claude` ayarlarına DCC hiçbir koşulda yazmaz.
- Windows, MVP'de yok. macOS'ta çalışıp sonra ayrı bir aşama olarak doğrulanır.
- Dağıtım, şimdilik yalnızca kendi kullanımın. İmzalama ve lisans kararı ertelenir.
- Git için lazygit, AI konfigürasyon senkronu için Skillshare, komut blokları MVP dışı, pane boyutları için yaklaşık geri yükleme. Bunlar PRD'de karar kaydı olarak yazılır.

## Düzeltmeler

Orijinal envanterde şu yerler spike bulgularıyla çelişiyordu veya eksikti.

1. **Dosya adı.** Görev metni `DCC-INVENTORY.md` diyordu, gerçek dosya `docs/inputs/DCC Yetenek Envanteri.md`. Aynı dosya kabul edildi.
2. **Terminallerin core'da yaşaması.** Orijinal satır "terminal uygulamadan bağımsız çekirdekte çalışır" diyordu ve kanıtı kaynak taramasıydı. Spike Adım 4'te core durdurulup başlatılınca adres ve klasör korundu ama shell yeni bir süreçti. Satır buna göre düzeltildi.
3. **Workspace kapanınca terminaller.** Adım 4'te kapatılan space'in 4 terminalinden 2'si yaşadı ve sonra fazladan bir space olarak göründü. Fazladan space'i hangi adımın yarattığı doğrulanmadı, satıra eklendi.
4. **Pane boyutları.** "Hiçbir API'den okunamıyor" doğrulandı. Herdr'ın kendi yeniden başlatmada boyutu koruyup korumadığı hiç sınanmadı, satıra "doğrulanmadı" olarak eklendi.
5. **Çekirdek değişikliği gerektiren işler.** Giriş paragrafı yalnızca iki iş sayıyordu. Olay akışı (`events.subscribe`) çekirdekte var ama mod API'sinde görülmedi, üçüncü aday olarak yazıldı ve "doğrulanmadı" diye işaretlendi.
6. **Durum takibi.** Orijinal satır "Native, Kolay" idi. Spike, durumun çekirdek durumundan alınması gerektiğini gösterdi (`agents()` pencereye göre `done` değerini `idle` yapıyor, risk 10). Notification hook'unun geç ve isteğe bağlı olduğu (risk 11) eklendi.
7. **Çoklu agent paneli.** Orijinal "Kolay" idi. Panel çıkan oturumu kaybettiği için (risk 20) ayrı bir not eklendi, oturum kaydı satırı bunu karşılıyor.
8. **Ortak durum modeli.** "Failed ve Stopped için kaynak sorunu aşağıda" cümlesi yerine spike sonucu yazıldı. Eşleme 9 testle denendi, Failed ve Stopped canlı gözlenmedi.
9. **Kota satırları.** Spike ölçümü eklendi. `rate_limits` alanı 12 çağrının 11'inde vardı, `used_percentage` ilk yanıtlara kadar null olabilir. Hiçbir sayı yazılmadı.
10. **Güven sorusu.** Orijinal satır Adım 5 ve 6'yı kanıt gösteriyordu. Herdr'ın durumu Adım 5'te gözlendi, panelin Unknown satırı Adım 6'da canlı görülmedi (soru çıkmadı). Satır buna göre düzeltildi.
11. **Eksik satırlar eklendi.** Çekirdek yaşam döngüsü (risk 14), agent ortamının temizlenmesi (risk 15), log alan beyaz listesi (risk 16, 17), test izolasyonu (risk 19). Orijinalde yoktu.
12. **Ortam sürümleri.** Orijinal yalnızca "Herdr 0.1.x" diyordu. Spike'ta ölçülen sürümler (uygulama 0.1.15, core 0.1.20) başa yazıldı. Spike raporunun başlığındaki "app ve core 0.1.20" ifadesi yanlıştı.
13. **Skillshare.** Güncel kaynak taraması (2026-11-12) lisansını MIT, son sürümü v0.30.6 olarak gösteriyor ve bir Herdr paneli tarif ediyor. Satır bu kaynağa dayandırıldı, çalıştırılarak doğrulanmadı.
