# DCC yol haritası

Sürüm 0.1, taslak. Dayanaklar `docs/02-PRD.md` (kapsam, kabul ölçütleri, karar kayıtları) ve `docs/03-architecture.md`. Spike'ta denenmeyen her yer "doğrulanmadı" diye işaretlidir.

## 1. Önerilen sıra ve bağımlılıklar

| Faz | Konu | Bağımlı olduğu | Sonunda gösterilebilen sonuç |
| --- | --- | --- | --- |
| 0 | Doğrulama testleri | Yok | Beş testin sonuç tablosu ve mimariye etkisi |
| 1 | Çekirdek | 0 (test 3 için) | Kendi layout'unla açılan Herdr, workspace kaydet ve core yeniden başlatıldıktan sonra geri yükle |
| 2 | Agent paneli ve oturum kaydı | 0 (test 1 ve 4), 1 | İki agent tek panelde, kapanan oturum listede son durumuyla |
| 3 | SSH | 0 (test 5), 1 | Profil ekle, bağlan, port yönlendir, kayıtlı komut çalıştır |
| 4 | Git ve kod sekmesi | 0 (test 2 ve 3), 1, 2 | Kod sekmesi şablonu ve odaktaki klasörde lazygit popup'ı |
| 5 | AI konfigürasyon | 1 | Agent başına skills, rules, MCP envanteri ve Skillshare çağrısı |
| 6 | Kullanım ve kota | 2 | Bağlam ve kota ayrı başlıklarda, kaynak yoksa Unavailable |
| 7 | Windows | 1 ile 6 | Her yeteneğin Windows'ta yeniden doğrulanmış listesi |

Sıranın gerekçesi. Çekirdek her şeyin altyapısı (SQLite, job, Keychain, layout). Agent paneli DCC'nin ana değeri olduğu için SSH, Git ve AI konfigürasyondan önce gelir. Kullanım ve kota agent kaydına bağlı olduğundan Faz 2'den sonra. Windows ayrı ve son, çünkü macOS'ta çalışmayan şey Windows'ta da çalışmaz.

Çalışma planındaki "Faz 1, 2, 3" adımları (dokümanlar, doğrulama testleri, uygulama) bu ürün fazlarından farklıdır. Çalışma planındaki Faz 2 bu yol haritasının Faz 0'ıdır.

## 2. Ortak kurallar

- Her faz kendi branch'inde ilerler (`phase-N-konu`), merge'i kullanıcı yapar.
- Her faz sonunda `cargo test` ve `cargo clippy` temiz, mod senaryo testleri izole ortamda geçiyor, kabul ölçütleri tek tek işaretleniyor. Geçmeyen ölçüt geçmiş gibi gösterilmez.
- Her Herdr ve hc komutu izolasyon değişkenleriyle çalışır. Ortam yüklü değilse çalışmaz.
- İlerleme `docs/PROGRESS.md` içine yazılır.
- Güven ve izin soruları kullanıcı tarafından verilir.
- Konuşma içeriği, terminal adları ve sırlar hiçbir log ve test çıktısına girmez.

## 3. Faz 0, doğrulama testleri

**Hedef.** MVP'den önce açık kalan beş soruyu izole ortamda, en küçük modelle ve kısa oturumlarla yanıtlamak. Sonuçlar `docs/06-validation.md` içine yazılır. Durum, tamamlandı (2026-11-12), beş test geçti, koşulları ve etkileri o dosyada.

**Görevler ve beklenen sonuçlar.**

1. `claude --settings` ile hook ve statusline enjeksiyonu. Ayar dosyası geçici bir yoldan verilir, `~/.claude` yazılmaz. Beklenen, hook'ların çalışması ve oturum kimliğinin (`--session-id`) hook'ta gelmesi.
2. Lazygit ve neovim'in Herdr pane'inde çalışması ve lazygit'in odaktaki panelin klasöründe popup olarak açılması (mod overlay ve `focused().cwd`). Kurulu değillerse kurulum komutu kullanıcıya yazılır, kendisi kurulmaz.
3. Kod sekmesi şablonunun (solda neovim, sağda lazygit, altta shell) layout olarak kurulması ve geri yüklenmesi.
4. Kapanan oturum için SQLite kaydı prototipi (Completed, Stopped, Failed ayrımı).
5. SSH port yönlendirme ve uzak makine bağlantısı. Gerçek sunucuya bağlanılmaz, localhost ssh ile denenir. Yapılamayan "denenmedi" yazılır.

**Modüller.** Yalnızca deneme betikleri ve küçük dcc denemeleri. Üretim kodu yok.

**Bağımlılıklar.** Spike klasörü (referans), izole ortam.

**Kabul ölçütleri.**

- Her test için beklenen, gerçekleşen ve geçti mi bilgisi `docs/06-validation.md` içinde.
- Mimariyi veya yol haritasını değiştiren sonuç ilgili dokümanda güncellenir ve `docs/PROGRESS.md` içine yazılır.

**Test planı.** Her test tek başına tekrarlanabilir. Reddedilen veya onay isteyen adımda durulur.

**Gösterilebilir sonuç.** Beş satırlık test sonuç tablosu ve mimariye etkisi.

## 4. Faz 1, çekirdek

**Hedef.** Üzerine her şeyin kurulacağı altyapı. Temiz bir dcc, SQLite, kendi layout, workspace kaydet ve geri yükle.

**Görevler.**

1. Spike dcc crate'ini yeni depoya taşı ve temizle. Cargo workspace, modüller (`secret`, `store`, `jobs`, `workspace`, `herdr`, `diag`), hata tipi, JSON çıktı kuralları. Spike'taki `agent`, `snapshot` kodu Faz 2'ye bırakılır. Spike klasörü değiştirilmez.
2. SQLite deposu. Göç sistemi (`user_version`), `workspaces` ve `contract_runs` tabloları, WAL, busy timeout.
3. Job deseni. `start`, `status`, `clean`, `sweep`, 0700 izin. Spike kodu temizlenerek alınır.
4. Gizli bilgi. `secret set` (yalnızca terminal), `exists`, `delete`. `get` komutu yazılmaz.
5. Herdr sarmalayıcısı. `herdr-app` ve `hc` çağrıları, izolasyon doğrulaması, çekirdek başlatma ve durdurma, PID ile kapanış doğrulaması. `hc server status` canlılık sınaması olarak kullanılmaz.
6. Kendi layout. Tarif dosyası, `dcc herdr ensure-layout`, `ensure-mods`, sürüm çifti kaydı (`compat.toml`).
7. Workspace kaydet ve geri yükle, iki modda. Şablon (bizim komut ağacımız, örneğin kod sekmesi) ve yaklaşık anlık kayıt. Geri yüklemede iki yol, core yeniden başlayınca programları yeniden yazma (`hc send`) ve space kapalıysa ağacı yeniden kurma. Space kapanırken terminallerin bilerek kapatılması veya park edilmesi ve doğrulanması (`space.close` tutarsız). Özel bölme oranları ve üç ve daha fazla pane denenir.
8. Geliştirme araçları. `xtask` ile izole ortam başlatma, çekirdek ve uygulama kapatma, sızıntı taraması. Oturum adları ve veri yolları kısa tutulur ve çekirdek soket yolu 103 bayt sınırına karşı başlangıçta denetlenir (Faz 0 test 5). Süreç kapanışları PID ile doğrulanır, ortam değişkenleri açıkça dışa aktarılır (zsh'te bölünmeyen dizgi hatası yaşandı).

**Modüller.** `crates/dcc`, `crates/xtask`, `layouts/`, `mods/dcc-workspace`.

**Bağımlılıklar.** Faz 0 test 3 (kod sekmesi layout'u Faz 4'te ama layout üretici bu fazda).

**Kabul ölçütleri.**

1. Workspace kaydedilip core yeniden başlatıldıktan sonra geri yüklenince aynı sekmeler, klasörler ve başlangıç komutları kuruluyor (spike'ta iki sekme ve üç komutla gösterilmişti, burada üç sekme ve üç pane ile).
2. Aynı adlı space açıkken restore reddediliyor, kayıtsız ad hata veriyor.
3. Layout üretici sürüm değişince yeniden üretiyor ve `herdr-app status` hata göstermiyor.
4. `secret get` komutu ve mod yüzeyi yok. Sızıntı taraması (düz metin, base64, argv, ekran) sahte sırla temiz.
5. 40 sn'lik job sırasında pencere donmuyor, sweep çalışan işe dokunmuyor.
6. Test sonunda izole çekirdek ve uygulama PID ile doğrulanmış kapalı.
7. `cargo test` ve `cargo clippy` temiz.

**Test planı.** Birim (SQLite göçleri, job durumları, workspace tanımı), entegrasyon (geçici repolar, gerçek SQLite), mod senaryosu (izole Herdr, ekran görüntüsü), güvenlik (sızıntı taraması).

**Gösterilebilir sonuç.** DCC layout'uyla açılan Herdr. Bir proje workspace'i kaydedilir, core kapatılıp açılır, tek komutla aynı sekmeler ve komutlar geri gelir.

## 5. Faz 2, agent paneli ve oturum kaydı

**Hedef.** DCC'nin ana değeri. Çoklu agent durumunu, kimliğini, branch ve worktree'sini, değişiklik sayısını tek panelde ve kapanan oturumlarla birlikte göstermek.

**Görevler.**

1. Adapter arayüzü ve Claude Code adapter'ı (`docs/03-architecture.md` bölüm 6). `GenericAdapter` yalnızca Herdr durumu ve klasör eşlemesi için.
2. `dcc agent exec` (ortam temizleme) ve `dcc agent launch` (worktree seçimi, oturum kimliği, geçici ayar dosyası, `--settings` ve `--session-id`). Test 1 `--settings` yolunun çalıştığını gösterdi. Agent `hc run --bg` ve `herdr.space`, `herdr.tab` etiketleriyle açılır, çünkü çıkış kodu yalnızca bu yolda gelir (test 4). Gerçek Claude Code ile bu yolun denenmesi ve etiketli sekmeyi almak için kullanılan boş sekmedeki shell pane'inin kapatılması bu fazın ilk işidir.
3. `hook-log` ve `statusline-log` komutları. Alan beyaz listesi, her zaman sıfır çıkış.
4. `snapshot --record`. Git bilgisi, kimlik (argv, hook, none), DCC durumu, SQLite yazımı. Kayıt iki katmanlıdır, `agent_runs` (süreç) ve `agent_sessions` (oturum). `/clear` aynı çalıştırmada yeni oturum kimliği üretir (test 1), yeni `SessionStart` yeni oturum satırı açar. Bitmiş çalıştırmaların çekirdek kaydından kapatılması (Completed, Stopped, Failed). DCC'nin gönderdiği kesme Stopped olarak kaydedilir.
5. OSC 7501 sözleşme testi (`dcc diag contract`) ve `dcc-fixture-agent`.
6. `dcc-agents` modu (salt okunur). Sıralama Waiting, Running, Unknown, Completed, Stopped, Failed. Çekirdek durumu kullanılır. Güven sorusu için Unknown notu. Kota alanı "Unavailable" (Faz 6'ya kadar).
7. Agent başına worktree otomasyonu (`git worktree add`), silme ayrı onayla.

**Modüller.** `crates/dcc` (agent, store), `crates/dcc-fixture-agent`, `mods/dcc-agents`.

**Bağımlılıklar.** Faz 0 test 1 ve 4, Faz 1.

**Kabul ölçütleri.**

1. İki agent aynı anda iki satır gösteriyor (farklı branch, farklı worktree etiketi).
2. Durum değişikliği panele medyan 3 sn içinde yansıyor (spike'ta 1,1 sn).
3. Agent çıkınca ve `hc kill` ile öldürülünce satır kayıttan geliyor, Completed ve Stopped ayrımı doğru. Failed için bir test (öldürülmüş değil, sıfır olmayan çıkışlı sahte süreç).
4. `--session-id` ile başlatılan agent'ın ilk oturum kimliği argv, hook ve kayıtta aynı. `/clear` sonrası yeni kimlik yeni oturum olarak kayda giriyor ve aynı çalıştırmaya bağlanıyor.
5. Hook ve statusline günlüklerinde konuşma içeriği yok (test), hook komutu her durumda sıfırla çıkıyor.
6. Sözleşme testi geçiyor, bozuk bir fixture ile "durum tespiti güvenilmez" moduna giriyor.
7. Aynı klasörde iki agent uyarı gösteriyor (risk 22).

**Test planı.** Birim (eşleme kuralları, kimlik sırası, alan beyaz listesi), entegrasyon (geçici repolar, sahte hook JSON), mod senaryosu (boş durum, iki agent, hook'suz agent, değişen dosya sayısı, çıkış), elle kontrol (gerçek Claude Code oturumu, güven ve izin soruları kullanıcı tarafından).

**Gösterilebilir sonuç.** Panelde iki canlı agent ve bir kapanmış oturum. Her satırda durum, kimlik, branch, worktree, değişiklik sayısı.

## 6. Faz 3, SSH

**Hedef.** Profil, gizli bilgi ve kayıtlı komutlarla güvenli bir SSH yöneticisi.

**Görevler.**

1. `ssh_groups`, `ssh_profiles`, `port_forwards`, `saved_commands` tabloları ve komutları.
2. Parola ve passphrase girişi gizli terminal panelinden. Sırrı alt sürece verme yolunun seçimi (askpass veya ortam) ve taramalı test.
3. Bağlanma (pane içinde ssh veya `hc machine`, test 5 sonucuna göre).
4. Port yönlendirme (local, remote, dynamic). Test 5'te üçü de `ssh -N` ile `hc run --bg` terminali olarak çalıştı ve `hc kill` ile durdu. Süreç çekirdek tarafından izlenir. Job klasörü deseni dcc'nin kendi iç işleri için kalır. Uzak makine için `hc machine add --via ... --session ...` kullanılır, uzak tarafta `hc` PATH'te olmalıdır.
5. Kayıtlı komutlar. Çalıştırma `hc send` ile, bloklu programa `--force` olmadan yazılmaz. Komut kaydında sır deseni taraması.
6. `dcc-ssh` modu (profil listesi, bağlan, yönlendirmeler, komutlar).

**Modüller.** `crates/dcc` (ssh, store), `mods/dcc-ssh`.

**Bağımlılıklar.** Faz 0 test 5, Faz 1 (Keychain, job).

**Kabul ölçütleri.**

1. Profil eklenir, parola Keychain'e girilir. Sızıntı taraması argv, log, SQLite ve ekranda temiz.
2. Bağlantı açılır (localhost ssh ile test, gerçek sunucuya bağlanılmaz).
3. Port yönlendirme job olarak çalışır, durumu paneldeki satırdan görülür ve durdurulur.
4. Kayıtlı komut seçilen terminalde çalışır.
5. Gerçek uzak makine bağlantısı (localhost dışı), parolayla giriş, `ssh-agent` ve SFTP dosya gezgini bu fazda denenmemiş veya kapsam dışı olarak işaretli.

**Test planı.** Birim (profil, şema), entegrasyon (localhost ssh), güvenlik taraması, mod senaryosu.

**Gösterilebilir sonuç.** Bir profil ekleme, bağlanma, yönlendirme açma ve kapatma akışı.

## 7. Faz 4, Git ve kod sekmesi

**Hedef.** Diff, stage, commit ve rebase için lazygit'i, düzenleme için neovim'i Herdr'a oturtmak. DCC'nin kendi değeri olan branch, worktree ve değişiklik bağını panellere taşımak.

**Görevler.**

1. Lazygit popup'ı. Odaktaki panelin klasöründe (`focused().cwd`), yeni pane için `terminal.open {folder, place}`. Test 2 sonucuna göre overlay veya pane.
2. "Kod sekmesi" şablonu. Solda neovim, sağda lazygit, altta shell, layout olarak ve workspace kaydına dahil.
3. `dcc git worktree add/list/remove`. Silme ayrı onay ister.
4. Git bilgisinin (branch, worktree, değişen dosya) diğer panellerle ortaklaşa kullanımı.
5. `dcc-code` modu (komutlar, popup, şablon açma).

**Modüller.** `crates/dcc` (git), `mods/dcc-code`, `layouts/`.

**Bağımlılıklar.** Faz 0 test 2 ve 3, Faz 1 ve 2. Lazygit ve neovim kurulu olmalı. DCC bir şey kurmaz, eksikse kurulum komutu gösterilir.

**Kabul ölçütleri.**

1. Kod sekmesi şablonu açılıyor ve workspace kaydedilip geri yüklenince geliyor (boyutlar yaklaşık).
2. Lazygit popup'ı odaktaki panelin repo'sunda açılıyor, başka klasörde farklı repo'yu gösteriyor.
3. Worktree oluşturma ve listeleme çalışıyor, silme onay istiyor.
4. DCC içinde kendi diff, stage veya commit arayüzü yok.

**Test planı.** Birim (git komutları, geçici repolar), mod senaryosu (popup klasörü, şablon geri yükleme), elle kontrol (lazygit ve neovim kullanımı).

**Gösterilebilir sonuç.** Bir tuşla kod sekmesi, bir tuşla lazygit popup'ı.

## 8. Faz 5, AI konfigürasyon

**Hedef.** AI araçlarının yapılandırmasını görmek ve Skillshare ile senkronlamak. Kendi senkron motoru yazılmaz (KK-06).

**Görevler.**

1. Envanter (salt okunur). Her agent için skills, rules, MCP, hook ve plugin dosyalarının yolu, türü ve boyutu. Dosya içeriği okunmaz veya gösterilmez, yalnızca üst bilgi.
2. Skillshare çağrısı. Durum, listeleme ve senkron, job deseniyle. Skillshare'in dry-run seçeneği varsa onu kullanan bir önizleme.
3. Güvenli yazma kuralı. Yapılandırma dosyasına yazan her eylem önce yedek, diff önizleme, dry-run ve silme için ayrı onay ister.
4. `dcc-config` modu.

**Modüller.** `crates/dcc` (config), `mods/dcc-config`.

**Bağımlılıklar.** Faz 1. Skillshare kurulu olmalı (kurulum kullanıcıya bırakılır).

**Kabul ölçütleri.**

1. Envanter agent başına dosya yollarını ve türlerini listeliyor, içerik göstermiyor.
2. Senkron önizlemesi yazmadan önce yedek ve diff gösteriyor.
3. Silme eylemi ayrı onay istiyor.
4. DCC `~/.claude` ayarlarına yazmıyor (test).

**Test planı.** Birim (envanter, yedek ve diff), geçici ev klasörüyle entegrasyon (gerçek `~` kullanılmaz), güvenlik (yazma yolları taraması).

**Gösterilebilir sonuç.** Envanter listesi ve Skillshare ile bir senkron önizlemesi.

## 9. Faz 6, kullanım ve kota

**Hedef.** Bağlam kullanımı ve hesap kotasını yalnızca resmi kaynaktan, ayrı başlıklarda göstermek.

**Görevler.**

1. `statusline-log` verisinden `usage_samples` kaydı (bağlam ve kota ayrı). Alan yoksa satır yazılmaz.
2. `dcc-usage` modu. İki ayrı bölüm, kaynak yoksa "Unavailable". Hiçbir hesaplama, tahmin veya çıkarım yok.
3. Geçmiş görünümü, yalnızca DCC çalışırken toplanan örneklerden.
4. Diğer agent'lar için resmi kaynak araştırması. Bulunana kadar "Unavailable".

**Modüller.** `crates/dcc` (usage, store), `mods/dcc-usage`.

**Bağımlılıklar.** Faz 2 (statusline kaydı ve oturum).

**Kabul ölçütleri.**

1. Bağlam ve kota ayrı başlıklarda.
2. DCC'nin başlatmadığı bir agent için kaynak yok ve "Unavailable" yazıyor.
3. Kodda ve veritabanında tahmin veya türetilmiş kota değeri yok (kod incelemesi ve test).
4. `used_percentage` null iken panel "bilinmiyor" diyor.
5. Gerçek hesapla etkileşim gerektiren bir sorgu çalıştırılmıyor.

**Test planı.** Birim (örnek statusline JSON'u, null ve eksik alanlar), mod senaryosu.

**Gösterilebilir sonuç.** Bir agent oturumunun bağlam kullanımı ve (varsa) kota alanı, kaynak yoksa Unavailable.

## 10. Faz 7, Windows

**Hedef.** macOS'ta çalışan her yeteneği Windows'ta yeniden doğrulamak. Bu faz kullanıcı istediğinde başlar.

**Görevler.**

1. Derleme ve temel testler (yol, süreç, dosya işlemleri).
2. Job ayırma (`creation_flags`), `CREATE_NO_WINDOW`, hook komut satırı tırnaklama, hook günlüğüne eşzamanlı ekleme.
3. Credential Manager ile gizli bilgi, gizli terminal girişi (ConPTY).
4. `dcc agent exec` ve Herdr'ın agent tanıması.
5. Yol karşılaştırması (`C:/` ve `\\?\`), `git` ve `claude` bulunması, `.exe` adları.
6. Her fazın kabul ölçütlerinin Windows'ta yeniden koşulması.

**Bağımlılıklar.** Faz 1 ile 6.

**Kabul ölçütleri.** Her yetenek için "Windows'ta çalıştı" veya "çalışmadı" kaydı. Test edilmeyen hiçbir şey "çalışıyor" diye yazılmaz.

**Gösterilebilir sonuç.** Yetenek başına Windows durumu tablosu.

## 11. Açık riskler (fazlara dağılım)

| Risk | İlgili faz |
| --- | --- |
| Layout kopyası güncellemede yeniden üretim (risk 1) | 1 |
| OSC 7501 belgesiz, sürüm sabitleme (risk 2) | 2 |
| Mod sandbox'suz, `get` yok (risk 5) | 1, 3 |
| İmzasız ikili ve Keychain (risk 6) | Dağıtım kararına kadar ertelendi |
| Çıkan oturumlar kayboluyor (risk 20) | 2 |
| `/clear` sonrası kimlik (risk 21) | 2 (Faz 0 test 1'de bir kontrol) |
| Aynı klasörde iki agent (risk 22) | 2, 4 |
| Hook günlüğü tek dosya (risk 23) | 2 |
| Polling gecikmesi ve kısa durumlar (risk 24) | 2 |
