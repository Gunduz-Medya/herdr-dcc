# DCC Yetenek Envanteri

Nov 12, 2026 · @Barış

## Okuma kılavuzu

Brief'teki özelliklerin hepsi Herdr üzerinde bir mod ve küçük bir Rust yardımcısıyla (dcc) yapılabilir görünüyor. Herdr çekirdeğini değiştirmek yalnızca iki iş için gerekiyor (pane boyutlarının kalıcılığı ve komut blokları), ikisi de MVP dışı önerilir. Bu sonuç spike (Adım 1 ile 6) ve kaynak taramasına dayanıyor. Spike'ta denenmeyen yerler açıkça işaretli, çünkü o satırlarda zorluk tahmindir.

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

Herdr 0.1.x sürüm çizgisinde. API kararlılığı taahhüdü belgelerde görülmedi, bu doğrulanmadı ve ayrı bir risktir.

## Terminal ve workspace

Sekme, split ve çoklu terminal Herdr'da hazır. Asıl iş, workspace'i kendi kuralımızla kaydedip geri yüklemek.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Sekmeler, dikey ve yatay split, tek ekranda çoklu terminal | Native | Kolay | Spike Adım 4 | Sekmeler ve klasörler geri yüklemede doğru geldi |
| Terminallerin core'da yaşaması | Native | Kolay | Kaynak taraması | Terminal uygulamadan bağımsız çekirdekte çalışır |
| Proje workspace'i (klasör, sekmeler, başlangıç komutları) kaydet ve geri yükle | dcc + Mod | Orta | Spike Adım 4 | Gönderilmiş layout'lar salt okunur, DCC kendi layout kopyasını taşır (risk 1) |
| Workspace kapanınca terminalleri kapatma veya park etme | dcc + Mod | Orta | Spike Adım 4, 5, 6 | space.close terminalleri kapatmıyor, hc kill gerekti |
| Pane boyutlarının (split oranı) kalıcı olması | Core Change | Zor | Spike Adım 4 | Hiçbir API'den okunamıyor. MVP'de yaklaşık geri yükleme kabul edilir |
| Neovim ve lazygit düzeni ("kod sekmesi" şablonu) | Native + Mod | Kolay | Kaynak taraması | Terminal paneli olarak çalışır, şablon layout olarak eklenir. Herdr'da denenmedi |
| Lazygit popup (odaktaki panelin klasöründe) | Mod | Orta | Kaynak taraması | TUI'deki HERDR\_ACTIVE\_PANE\_CWD benzeri değişken yok, mod focused().cwd kullanır. Denenmedi |
| Pane bölerken klasör verme | Mod | Kolay | Kaynak taraması | pane.split'te klasör ve odaksız bölme yok, terminal.open {folder, place} ile aşılır |
| Tuş atamaları ve komut tetikleme | Native | Kolay | Kaynak taraması | keys.json, mod komutlarını bağlayabilir |
| Komut blokları, çıkış kodu işareti | Core Change | Orta | Denenmedi | Kabuk entegrasyonu ve uygulama içinde komut sınırı takibi gerekir. MVP dışı |
| Komut blokları, son çıktıyı kopyala | Core Change | Orta | Denenmedi | Aynı altyapı. MVP dışı |
| Komut blokları, çıktıyı katlama | Core Change | Zor | Denenmedi | Çizim ve girdi editörü değişir. Değer düşük bulundu, ertelendi |

## SSH yöneticisi

Profil, Keychain ve uzun iş altyapısı spike'ta çalıştı. Port yönlendirme ve SFTP hiç denenmedi, buradaki zorluklar tahmindir.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Uzak makineye bağlanma (uzakta terminal açma) | Native | Orta | Kaynak taraması | herdr-core makineleri SSH üzerinden bağlayabiliyor (hc machine). Uzak tarafta ne gerektiği doğrulanmadı |
| SSH profilleri ve grupları (ad, host, kullanıcı, port, etiket) | dcc + Mod | Kolay | Spike Adım 3, 4 | Kayıtlar SQLite'ta, panel modda. SQLite yazma ve okuma çalıştı |
| Anahtar ve parola saklama (macOS Keychain) | dcc | Orta | Spike Adım 2, 3a | Gizli bilgi argv'ye ve mod yüzeyine çıkmaz. İmzasız ikili sorunu denenmedi (risk 6) |
| Anahtar üretme ve ssh-agent ile kullanma | Dış araç | Orta | Denenmedi | ssh-keygen ve ssh-agent'ı dcc sarmalar |
| Port yönlendirme (local, remote, dynamic) | dcc | Orta | Denenmedi | ssh süreci olarak çalışır, uzun işler için kullandığımız job deseni uygulanır (Spike Adım 4b) |
| SFTP, terminalden (sftp, rsync) | Dış araç | Kolay | Denenmedi | Pane içinde çalışır |
| SFTP, dosya gezgini ve sürükle bırak | Mod | Zor | Denenmedi | Mod arayüzü, aktarım kuyruğu ve ilerleme gerekir. MVP dışı önerilir |
| Kayıtlı komutlar (snippet) | Mod + dcc | Kolay | Spike Adım 3 | Komutlar SQLite'ta, çalıştırma hc send ile. hc send bloklu programa --force olmadan yazmaz |
| Parola ve passphrase girişi | Mod + dcc | Kolay | Spike Adım 2, 3a | exec'te stdin yok, giriş gizli bir terminal panelinden alınır |

## Git yöneticisi

DCC'nin kendi değeri agent ile branch, worktree ve değişiklik arasındaki bağ. Diff, stage ve rebase için lazygit'i panelde çalıştırmak, sıfırdan Git arayüzü yazmaktan çok daha hızlı.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Branch, worktree ve değişen dosya sayısı | dcc | Kolay | Spike Adım 5, 6 | Klasör bazında güvenilir (main, ayrı worktree, detached, alt klasör, silinmiş worktree) |
| Agent ile branch, worktree ve değişiklik bağı | dcc + Mod | Orta | Spike Adım 5, 6 | Aynı klasördeki iki agent ayrılamaz (risk 22) |
| Agent başına ayrı worktree otomasyonu | dcc | Orta | Denenmedi | Risk 22'nin önerilen çözümü. git worktree add çağrısını dcc yapar |
| Worktree oluşturma, silme, listeleme | dcc | Kolay | Spike Adım 5 | Spike'ta elle kuruldu, dcc ile otomatik denenmedi |
| Diff görüntüleme, stage ve unstage, commit | Dış araç (lazygit) | Kolay | Kaynak taraması | Herdr'da denenmedi. Dış kurulum rehberinde pane veya popup olarak kullanılıyor |
| Branch, stash, rebase, çatışma çözme | Dış araç (lazygit) | Kolay | Kaynak taraması | Lazygit kapsıyor, bizim kodumuz gerekmez |
| Lazygit'i odaktaki panelin repo'sunda açma | Mod | Orta | Kaynak taraması | focused().cwd ile. Yeni pane için terminal.open {folder, place} kullanılır. Denenmedi |
| Kendi Git arayüzü (Gitfox benzeri diff, stage, rebase) | Mod | Zor | Denenmedi | gpui-kit ile çizilir. Büyük iş, MVP dışı önerilir |

## Agent yönetimi

Claude Code durumu ve kimliği spike'ta güvenilir çıktı. Kapanan oturumlar için DCC'nin kendi kaydı şart, diğer agent'lar ise hiç denenmedi.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Durum takibi (Claude Code) | Native | Kolay | Spike Adım 5, 6 | Herdr durumu Claude'un OSC 7501 raporundan alıyor, bu kanal belgesiz (risk 2) |
| Durum takibi (Codex, Gemini CLI, Cursor, Grok) | Native veya dcc | Orta | Denenmedi | Bu agent'ların OSC 7501 yayınlayıp yayınlamadığı bilinmiyor. Yayınlamıyorlarsa dcc adapter'ı gerekir |
| Ortak durum modeli (Running, Waiting, Completed, Failed, Stopped, Unknown) | dcc | Kolay | Spike Adım 6 | Eşleme kuralı yazıldı ve test edildi. Failed ve Stopped için kaynak sorunu aşağıda |
| Oturum kimliği | dcc | Kolay | Spike Adım 5, 6 | DCC agent'ı --session-id ile başlatırsa kimlik Herdr argv'sinde ve hook'ta aynı çıktı. Dışarıdan başlatılanlarda yok |
| Agent başlatma (worktree, ayar ve kimlikle) | dcc + Mod | Orta | Denenmedi | hc run veya terminal.open kullanılır. --settings ile hook enjeksiyonu test edilmedi, yol haritasının ilk küçük testi |
| Oturum kaydı (kapanan oturum, son durum, branch, değişiklik) | dcc | Orta | Spike Adım 6 (boşluk) | panes() çıkan oturumu kaybediyor. Completed, Stopped ve Failed için DCC'nin kendi SQLite kaydı şart (risk 20) |
| Güven sorusu bekleyen agent | dcc + Mod | Kolay | Spike Adım 5, 6 | Herdr durum göstermiyor. DCC "Unknown, güven sorusu olabilir" gösterir, kendisi onaylamaz |
| Bildirim (izin bekliyor, bitti) | dcc + Mod | Orta | Spike Adım 6 | Polling ile kısa durumlar kaçıyor (19 geçişin 3'ü). Çekirdekte events.subscribe var, mod API'sinde yok |
| Agent'a prompt gönderme ve çıktı okuma | Mod | Kolay | Kaynak taraması | hc send ve hc read. Çıktı konuşma metni içerebilir, loglanmaz |
| Çoklu agent paneli (durum, kimlik, branch, worktree, değişen dosya) | Mod | Kolay | Spike Adım 6 | Salt okunur panel çalıştı, medyan gecikme 1,1 sn |

## AI konfigürasyon yöneticisi

Bu bölüm spike'ta hiç denenmedi. Zorluklar Skillshare kaynak taramasına ve agent'ların yapılandırma düzenine dayanan tahmindir.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Skills, rules, MCP, hook ve plugin envanteri (okuma ve listeleme) | dcc | Orta | Denenmedi | Her agent'ın yapılandırma yolu ve biçimi ayrı. Salt okunur başlanır |
| Agent'lar arası senkron (tek kaynaktan skills, rules, MCP) | Dış araç veya dcc | Orta | Kaynak taraması | Skillshare'in kendi Herdr modu var (uzun işler için jobs.js deseni). Kendi senkron motorumuzu yazmak yerine onu çağırmak açık seçenek, karar gerekir |
| Güvenli yazma (yedek, diff önizleme, dry-run, silme için ayrı onay) | dcc | Orta | Denenmedi | Zorunlu kural. Yapılandırma dosyalarına yazan her özellik bunu önce sağlar |
| Claude global ayarlarına (\~/.claude) yazma | Kapsam dışı | Yok | Karar | MVP'de yok. DCC kendi başlattığı agent'a --settings verir |
| Codex, Cursor, Gemini CLI için hook ve plugin yönetimi | dcc | Zor | Denenmedi | Her agent'ın ayar şeması farklı ve sürümle değişebilir |

## Kullanım ve kota panosu

Resmi kaynak olan tek yer çalışan oturumun statusline verisi. Bunun dışında hiçbir rakam tahmin edilmez, kaynak yoksa panoda Unavailable yazar.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Oturum bağlam kullanımı (context penceresi) | dcc | Orta | Spike Adım 5 | Yalnızca çalışan oturumun statusline JSON'unda (context\_window alanları). Anahtar yolları okundu, sayılar işlenmedi |
| Hesap kotası (5 saatlik ve 7 günlük limit) | dcc | Orta | Spike Adım 5 | rate\_limits alanları yalnızca çalışan oturumun statusline'ında. Bağımsız ve programatik bir kota sorgusu bulunamadı. Abonelik dışı hesaplarda alanın varlığı doğrulanmadı |
| Diğer agent'lar için kullanım ve kota | dcc | Belirsiz | Denenmedi | Resmi kaynak araştırılmadı. Bulunana kadar Unavailable |
| Geçmiş kullanım grafiği | dcc | Kolay | Denenmedi | Yalnızca DCC çalışırken kaydedilen örneklerden oluşur, geriye dönük veri yok |
| Tahmin ve hesaplama | Yok | Yok | Karar | Resmi kaynak yoksa Unavailable yazılır. Bağlam kullanımı ile hesap kotası ayrı gösterilir |

## Altyapı ve platform

Altyapı parçalarının çoğu spike'ta macOS'ta çalıştı. Windows'ta hiçbir şey çalıştırılmadı.

| Özellik | Yol | Zorluk | Kanıt | Not |
| --- | --- | --- | --- | --- |
| Dış program çağırma | Mod | Kolay | Spike Adım 2, 3, 4a, 6 | exec çağrıları 13 ile 90 ms. 30 sn sınırı, stdin ve env yok |
| Uzun işler (detached job) | dcc | Orta | Spike Adım 3, 4b | 41 sn'lik iş sırasında pencere donmadı, başlatan süreç öldürülünce iş yaşadı. Yalnızca macOS, Windows kolu derlenmedi |
| Gizli bilgi (Keychain) | dcc | Orta | Spike Adım 2, 3a | get komutu mod yüzeyine açılmaz, yalnızca exists ve sırrı kendi içinde kullanan komutlar açılır (risk 5) |
| Yerel veri (SQLite) | dcc | Kolay | Spike Adım 3, 4 | WAL modunda terminal ve iş aynı anda yazdı. Yük testi yok |
| Üçüncü taraf mod güvenliği | Politika | Orta | Kaynak taraması | Mod kodu sandbox'suz çalışıyor. Şimdilik yalnızca kendi modlarımız |
| OSC 7501 sözleşmesi için sürüm sabitleme ve test | dcc | Orta | Spike Adım 5 | Biçim belgesiz (risk 2). Her Herdr sürümünde kısa bir sözleşme testi çalışır |
| DCC modunun kurulumu ve güncellenmesi | dcc | Kolay | Kaynak taraması | Masaüstünde plugin install yok. Mod klasörü \<app home>/mods altına kopyalanır, herdr-app check ile doğrulanır |
| Herdr güncellemesinde layout kopyasının yeniden kurulması | dcc | Orta | Spike Adım 4 | Gönderilmiş layout'lar salt okunur (risk 1) |
| Windows | Hepsi | Belirsiz | Denenmedi | MVP macOS. Windows ayrı aşama, her yetenek orada yeniden doğrulanır |
| Kod imzalama, güncelleme, dağıtım | Ertelendi | Orta | Karar | Şimdilik yalnızca kendi kullanımın. Keychain imza sorunu (risk 6) dağıtım kararına kalır |

## Açık noktalar ve sıradaki adım

Spike sonrası ürün yapılabilir görünüyor. Beş yerin MVP'den önce test edilmesi, dört yerin PRD'de karara bağlanması gerekiyor.

### Verilen kararlar

- Agent verisi, yalnızca DCC'nin başlattığı agent'larda --settings ve --session-id ile alınır. \~/.claude ayarlarına yazılmaz.
- Windows, MVP'de yok. macOS'ta çalışıp sonra ayrı bir aşama olarak doğrulanır.
- Dağıtım, şimdilik yalnızca kendi kullanımın. İmzalama ve lisans kararı ertelenir.

### MVP'den önce denenmesi gerekenler

1. --settings ile hook ve statusline enjeksiyonu (agent verisi kararının dayanağı)
2. Lazygit ve neovim'in Herdr pane'inde ve popup olarak çalışması
3. Uzak makine bağlantısı ve SSH port yönlendirme
4. Codex ve diğer agent'ların durum bildirimi (OSC 7501 yayınlıyorlar mı)
5. Kapanan oturumlar için DCC'nin SQLite oturum kaydı (Completed, Stopped, Failed)

### PRD'de karara bağlanacaklar

- Pane boyutları için yaklaşık geri yüklemeyi kabul etmek mi, Herdr'ı fork'lamak mı
- Git: lazygit'i kullanmak mı, kendi Git arayüzünü yazmak mı (öneri lazygit)
- AI konfigürasyon senkronu: Skillshare'i çağırmak mı, kendi motorumuzu yazmak mı
- Komut blokları: MVP dışı mı bırakılsın, çıkış kodu işareti ve son çıktıyı kopyala sonraya mı alınsın

Sıradaki adım PRD. Bu envanter onaylanırsa PRD'nin kapsam ve MVP sınırı buradan çıkar.
