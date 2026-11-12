# Faz 0 doğrulama testleri

Tarih 2026-11-12. Tüm testler izole ortamda yapıldı (`HERDR_APP_HOME`, `HC_SESSION=dcc-dev`, scratch XDG klasörleri, hepsi `~/dev/dcc/.dev` altında ve git'e girmiyor). Claude Code en küçük modelle (`haiku`), kısa ve araçsız promptlarla çalıştırıldı. Konuşma içeriği, terminal adları ve sırlar hiçbir çıktıya yazılmadı.

Ortam. macOS Apple Silicon, herdr-app 0.1.15, herdr-core 0.1.20, Claude Code 2.1.322, lazygit 0.64.0 (ekran görüntüsünden), OpenSSH 9.6p1. Neovim sürümü kaydedilmedi. Hook ve statusline komutları için spike'taki `dcc` ikilisi (salt okunur referans, spike klasörü değiştirilmedi) `--log-dir` ile bu klasöre yazdırıldı.

## 1. Sonuç tablosu

| Test | Beklenen | Gerçekleşen | Geçti mi |
| --- | --- | --- | --- |
| 1. `claude --settings` ile hook ve statusline | Hook'lar çalışır, oturum kimliği gelir, `~/.claude` değişmez | Altı hook olayından dördü (SessionStart, UserPromptSubmit, Stop, SessionEnd) ve statusline çalıştı, repoda hiçbir proje ayarı yokken. `session_id` verilen `--session-id` ile aynı. `settings.json` özeti ve zamanı aynı. Ek bulgu, `/clear` aynı süreçte yeni oturum kimliği üretiyor | Geçti |
| 2. Lazygit ve neovim pane'de, lazygit popup | İkisi pane'de çalışır, popup odaktaki panelin klasöründe açılır | Neovim ve lazygit pane'de çalıştı. Popup (mod, `focused().cwd`, `hc run` ve pencere overlay'i) odaktaki shell'in reposunda açıldı ve lazygit'ten çıkınca kapandı. Modun yüklü olması gerekiyor | Geçti |
| 3. Kod sekmesi şablonu | Üç pane'li sekme kurulur ve geri yüklenir | Komut dizisiyle kuruldu (üstte neovim ve lazygit, altta shell). Core yeniden başlayınca Herdr yapıyı ve klasörleri kendisi korudu, programlar kayboldu. Programlar `hc send` ile geri geldi. Space kapatılıp aynı komut dizisiyle yeniden kurulunca aynı düzen çıktı | Geçti (koşullu, bölüm 2.3) |
| 4. SQLite oturum kaydı, Completed, Stopped, Failed | Üç durum ayrışır | `hc run` ile açılanda çıkış kodu 0, 3 ve öldürme ayrıştı, kayıt tekrar çalıştırmada çoğalmadı. `terminal.open` ile açılanda çıkış kodu gelmiyor | Geçti (koşullu, bölüm 2.4) |
| 5. SSH port yönlendirme ve uzak makine | Yönlendirme ve uzak bağlantı çalışır | Pane'de ssh, local, remote ve dynamic yönlendirme ve `hc machine add --via` ile uzak makine (loopback sshd) çalıştı. Gerçek sunucuya bağlanılmadı | Geçti (yalnızca localhost) |

## 2. Ayrıntılar

### 2.1 Test 1, `claude --settings`

Kurulum. Geçici bir repo (`.dev/t1-repo`, `.claude` klasörü yok) ve geçici bir ayar dosyası (`.dev/t1/settings.json`, altı olay için `hook-log` ve bir `statusLine` komutu). Agent şu biçimde başlatıldı.

```
env -u CLAUDECODE ... claude --model haiku --permission-mode default \
    --setting-sources project,local --settings <ayar dosyası> --session-id <uuid>
```

Gözlenenler.

- Hook olayları geldi ve her birinde `session_id`, `cwd` ve `transcript_path` vardı. `session_id`, `--session-id` ile verilen değerle aynı. `transcript_path` `~/.claude/projects` altında.
- `SessionStart` anahtarları `source` içeriyor. `SessionEnd` anahtarları `reason` içeriyor (değerleri günlüğe yazılmıyor).
- Statusline 5 kez çalıştı. `session_id`, `model.id`, `context_window.used_percentage`, `context_window.context_window_size`, `cost.total_cost_usd` ve `workspace.current_dir` her çağrıda vardı. `rate_limits.five_hour` ve `rate_limits.seven_day` 5 çağrının 4'ünde vardı. Hiçbir sayı kaydedilmedi.
- `~/.claude/settings.json` sha256 ve değişiklik zamanı öncesi ve sonrası aynı. `settings.local.json` oluşmadı.
- `/clear` (risk 21 denemesi). Aynı süreçte `SessionEnd` ve ardından yeni bir `session_id` ile `SessionStart` geldi. Başlangıçta verilen `--session-id` bu noktadan sonra geçerli oturumu göstermiyor.

Sınırlar. `--settings` yalnızca `--setting-sources project,local` ile denendi. Kullanıcı ayar kaynağı açıkken ve kullanıcının kendi hook'ları varken ne olduğu denenmedi. Güven sorusu bu klasör için çıktı ve kullanıcı tarafından onaylandı (DCC onay vermez).

### 2.2 Test 2, lazygit ve neovim

- Neovim `space.new {argv}` ile açıldı. Ön plandaki program `nvim`, klasör doğru, ekranda dosya adı ve satır işaretleri vardı.
- Lazygit `terminal.open {argv, folder, place:"right"}` ile açıldı. Ön plandaki program `lazygit`, ekranda Status, Files, branch ve değişen dosyalar vardı.
- Popup. Test modu (`popup-test`) `focused().cwd` değerini okur, `hc(["run","--bg","--json","--cwd",<klasör>,"lazygit"])` ile lazygit'i ayrı bir çekirdek terminali olarak başlatır ve `openOverlay({kind:"window", layout:{type:"block", content:{terminal:<adres>}}})` ile gösterir. Başka bir repoda odaktaki shell ile çağrıldığında lazygit o repoyu açtı (`feature/popup-b` branch'i ve doğru dosya durumu ekran görüntüsünde göründü). Lazygit'ten `q` ile çıkınca overlay kendiliğinden kapandı.
- Modun yüklü olması gerekiyor. Yüklü değilken `herdr-app do popup-test.lazygit-popup` "unknown command ... need the mod loaded" hatası verdi. `mod.open` ile yüklenince çalıştı ve açık bir overlay kaydı olarak kaldı. Ürün için modun layout içinde kalıcı yüklü olması gerekir, bu yol bu testte sınanmadı.
- `hc run` ile açılan lazygit terminali etiketsiz olduğu için Herdr'ın ekranlarına girmedi (`herdr-app ls` göstermiyor, `hc ls` gösteriyor).
- Lazygit ilk açılışta bir karşılama diyaloğu gösterdi (scratch ortamda ilk çalıştırma).
- Neovim ve lazygit'in gerçek kullanımı (düzenleme, commit, rebase) denenmedi.

### 2.3 Test 3, kod sekmesi

"Şablon" bir layout dosyası değil, bir komut dizisi.

1. `space.new {name, folder, tab, argv:["nvim","README.md"]}` ilk pane'i açar.
2. `terminal.open {folder, place:"down"}` altta shell açar.
3. `pane.focus {pane:<ilk pane>}` odağı üst pane'e verir.
4. `terminal.open {argv:["lazygit"], folder, place:"right"}` üst pane'in sağına lazygit açar.

Sonuç, üstte `neovim | lazygit`, altta tam genişlikte shell (ekran görüntüsü alındı). `herdr-app ls` pane sırasını ağaç sırasıyla verdi (neovim, lazygit, shell).

Geri yükleme iki yolla denendi.

- **Core yeniden başlatma.** Herdr yapıyı (üst iki pane, alt shell), sekme adını, terminal adreslerini ve klasörleri kendisi korudu. Üç terminal de boş shell oldu. Programlar `hc send` ile ("nvim README.md" ve "lazygit") geri geldi ve düzen bozulmadı.
- **Space kapatıp yeniden kurma.** `space.close` sonrası aynı dört adım yeniden çalıştırıldı ve aynı düzen çıktı (ekran görüntüsü alındı). Bu testte `space.close` üç terminalin üçünü de kapattı. Spike Adım 4'te 4 terminalden 2'si yaşamıştı, fark nedeni bilinmiyor. Her seferinde doğrulanmalı.

Sınırlar. Yalnızca varsayılan (yarı yarıya) bölme oranları denendi, özel oranlar denenmedi. Spike'taki `dcc workspace save` yalnızca "sağa bölme" kaydediyor ve düzeni çıkaramaz (Herdr API'si bölme yönünü vermiyor). Şablon bu yüzden bizim yazdığımız bir ağaç olmalı, rastgele bir workspace'in yaklaşık kaydı ise sıralı pane listesi olarak kalır.

### 2.4 Test 4, oturum kaydı

Sahte agent (`fixture.py`) OSC 7501 ile `working` bildirir, bekler ve verilen koduyla çıkar. Çekirdekten okunan sonuçlar.

| Açılış yolu | Olay | Çekirdek kaydı | DCC durumu |
| --- | --- | --- | --- |
| `hc run --bg` | Çıkış kodu 0 | `finished`, `exit_code 0`, `reason exited` | Completed |
| `hc run --bg` | Çıkış kodu 3 | `finished`, `exit_code 3`, `reason exited` | Failed |
| `hc run --bg` | `hc kill` | `finished`, `reason killed`, `signal 1`, çıkış kodu yok | Stopped |
| `herdr-app do space.new {argv}` | Çıkış kodu 3 | Terminal canlı kalıyor (shell), çıkış kodu yok, durum temizleniyor | Ayrılamaz |
| `hc run --bg` ve `herdr.space`, `herdr.tab` etiketleri | Çıkış kodu 3 ve 4 | Terminal canlıyken sekmede göründü, bitince panelden kayboldu, çekirdekte `exit_code` kaldı | Failed |

Prototip (`session_store.py`, Python, atılabilir) bu kayıtları `agent_sessions` benzeri bir tabloya yazdı. Beş satır oluştu ve ikinci çalıştırma satır sayısını değiştirmedi.

Sonuç ve sınırlar.

- Completed, Stopped ve Failed ayrımı yalnızca çekirdeğin terminal kaydından çıkıyor ve bu kayıt yalnızca program doğrudan `hc run` ile açıldığında var. `terminal.open` shell ile sardığı için program çıkınca terminal canlı kalıyor.
- Etiketli `hc run` terminali Herdr sekmesine giriyor. Bu yol DCC'nin agent başlatması için aday.
- Gerçek Claude Code `hc run` ile denenmedi (çekirdeğin PATH ve ortamı, güven sorusu, `keep` davranışı belirsiz).
- Oturum sonu için ikinci kaynak `SessionEnd` hook'u (`reason` anahtarı var, değerleri denenmedi).

### 2.5 Test 5, SSH

Kurulum. Yalnızca `127.0.0.1:2222` üzerinde dinleyen, kendi anahtarlarımızla (`.dev/ssh`) çalışan geçici bir `sshd`. İstemci `-F /dev/null` ve `IdentityAgent=none` ile çalıştı, `~/.ssh` dosyalarına dokunulmadı. Uzak oturumun ortamı `SetEnv` ile scratch klasörlerine yönlendirildi.

- **Pane'de ssh.** `space.new {argv:["ssh", ...]}` ile açıldı, ön plandaki program `ssh`, uzak oturumda ortam değişkenleri doğruydu.
- **Local yönlendirme.** `ssh -N -L` bir `hc run --bg` terminali olarak çalıştı. Yönlendirme açıkken HTTP isteği uzak hedefe ulaştı, `hc kill` sonrası kapandı (terminal kaydı `finished`, `killed`).
- **Remote ve dynamic yönlendirme.** `-R` ve `-D` (SOCKS) aynı yöntemle çalıştı.
- **Uzak makine.** `hc machine add --name ... --session ... --via "<ssh komutu>"` makineyi ekledi ve `connected` oldu. `hc exec --on`, `hc run --on` ve `herdr-app do space.new {machine}` uzak terminal ve space açtı. Makine bu testte aynı bilgisayardı, uzak çekirdek ayrı bir oturum adı ve scratch klasörleriyle çalıştı.

Bulgular.

- Çekirdek soket yolu 103 bayt sınırına sahip. Uzun `HC_SESSION` adı ve derin XDG yolu çekirdeğin başlamasını engelledi (118 bayt, hata mesajıyla). Yollar ve oturum adları kısa tutulmalı.
- Uzak tarafta `hc` bulunmalı ve ssh oturumunun PATH'inde olmalı. Test ortamında bu `SetEnv PATH` ile sağlandı. `hc machine add --install` seçeneği var, denenmedi.
- Yönlendirmeleri `hc run --bg` ile yürütmek (çekirdek tarafından izlenen, `hc ls` ile görünen, `hc kill` ile durdurulan süreç) job klasörü desenine bir seçenek.

Denenmeyenler. Gerçek bir uzak makine, parolayla giriş, `ssh-agent` ve anahtar üretme, SFTP, `hc machine update`, uzak makinede lazygit veya agent çalıştırma.

## 3. Mimariye ve yol haritasına etkisi

| Bulgu | Etki | Güncellenen doküman |
| --- | --- | --- |
| `/clear` yeni oturum kimliği üretiyor | Bir süreç birden çok oturum taşıyabilir. Kayıt "çalıştırma" ve "oturum" olarak ikiye ayrılır | `03-architecture.md` bölüm 3.1, 5, `05-roadmap.md` Faz 2 |
| Çıkış kodu yalnızca `hc run` ile açılan terminalde var | DCC agent'ı `hc run` ve `herdr.space`, `herdr.tab` etiketleriyle başlatır | `03-architecture.md` bölüm 3.2, `05-roadmap.md` Faz 2 |
| Herdr yapıyı kendisi koruyor, programlar kayboluyor. Bölme yönü okunamıyor | Workspace için iki mod: şablon (bizim ağacımız) ve yaklaşık anlık kayıt. Geri yüklemede "programları yeniden yaz" yolu | `03-architecture.md` bölüm 3.3, `05-roadmap.md` Faz 1 ve 4 |
| Mod komutları için modun yüklü olması gerekiyor | `dcc-code` ve benzeri modlar layout tarifine eklenir | `03-architecture.md` bölüm 8 |
| Soket yolu 103 bayt sınırı | Oturum adı ve veri klasörü yolları kısa tutulur | `03-architecture.md` bölüm 10, `05-roadmap.md` Faz 1 |
| Yönlendirmeler `hc run --bg` ile yönetilebiliyor | SSH yönlendirmeleri bu yolla, job deseni dcc iç işleri için | `05-roadmap.md` Faz 3 |
| `space.close` bazen terminalleri kapatmıyor | Her kapanışta terminaller doğrulanır ve kapatılır | `03-architecture.md` bölüm 3.3 |

## 4. Denenmeyenler ve açık kalanlar

- `--settings` ile kullanıcı ayar kaynağı açıkken davranış. Kullanıcının kendi hook'ları ile birleşmesi.
- Gerçek Claude Code'un `hc run` ile açılması ve `SessionEnd` `reason` değerleri.
- Popup modunun layout içinde kalıcı yüklenmesi. Neovim ve lazygit'in gerçek kullanımı.
- Özel bölme oranlarının geri yüklenmesi. `space.close` davranışının tutarsızlığı.
- Gerçek bir uzak makine, SFTP, parola ve `ssh-agent`.
- Windows (hiçbir şey denenmedi).

## 5. Yan etkiler ve hatalar

- Dışarıya yazılanlar yalnızca Claude Code'un kendi dosyaları. `~/.claude.json` (güven kaydı) ve `~/.claude/projects` altında test reposu için bir oturum klasörü. Bunlar `docs/CLEANUP.md` içinde listelendi, silinmedi. `~/.claude/settings.json` değişmedi.
- Hata 1. Test 3'te core'u yeniden başlattım, bu da Test 1'in güven sorusunda bekleyen Claude sürecini öldürdü. Süreç yeniden başlatıldı ve soru ikinci kez onay istedi.
- Hata 2. Uzak çekirdeği durdururken ortam değişkenlerini zsh'te bölünmeyen bir dizgiyle verdim, komut yanlışlıkla yerel scratch çekirdeğini durdurdu. Uzak çekirdek (soket klasöründen ve çalışma süresinden tanındı) ayrıca doğru ortamla durduruldu. Tüm test süreçleri (uygulama, iki çekirdek ve holder'ları, sshd, http sunucusu) PID ile doğrulanmış kapalı. Kalan `herdr` süreçleri gerçek kurulumun.
- Hata 3. İlk sahte agent denemesi zsh'in kelime bölmemesi yüzünden bozuk argümanlarla koştu, düzeltilip tekrarlandı.
