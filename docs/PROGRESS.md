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

## Faz 1, çekirdek (2026-11-12, branch `phase-1-core`)

- Ne yapıldı.
  - Cargo workspace (`crates/dcc` kütüphane ve ikili, `crates/xtask`). Modüller `store`, `secret`, `jobs`, `workspace`, `herdr` (api, isolation, compat, layout, mods, core, fake), `error`, `util`. Spike'taki `agent` ve `snapshot` kodu Faz 2'ye bırakıldı, spike klasörü değiştirilmedi.
  - SQLite deposu (göç sistemi, `workspaces`, `contract_runs`, `herdr_state`, WAL, busy timeout). Job deseni (`start`, `status`, `clean`, `sweep`, 0700). Gizli bilgi (`set` yalnızca terminal, `exists`, `delete`, `get` yok).
  - Herdr sarmalayıcısı (trait ve gerçek CLI uygulaması, test için sahte Herdr), izolasyon denetimi (`DCC_REQUIRE_ISOLATION=1`, çekirdek soket yolu 103 bayt denetimi, formül Faz 0'daki 118 baytlık gözlemle doğrulandı), `compat.toml`, kendi layout'u (`layouts/dcc.recipe.json`, `ensure-layout`), mod kurulumu (`ensure-mods`, mod için `config.json`), çekirdek (`start`, `pids`, `stop`, `verify-closed`).
  - Workspace. Tanım bir bölme ağacı. İki tür (şablon ve anlık kayıt), kur (`restore`), yeniden doldur (`rehydrate`) ve kapat (`close`, `space.close` terminalleri bırakırsa sonlandırır). `code` şablonu. `dcc-workspace` modu (liste, Restore ve Rehydrate düğmeleri, odaktaki space'i kaydet).
  - `xtask` (`env`, `dev-up`, `dev-down`, `leak-scan`, `secret-leak`).
- Test sonucu.
  - `cargo test` 48 birim, 8 CLI entegrasyon, 3 xtask testi geçti. `cargo clippy --all-targets` temiz, `cargo fmt` uygulandı.
  - İzole Herdr senaryosu (gerçek pencere ve core, `HC_SESSION=dcc-dev`).
    1. 3 sekmeli ve 4 pane'li workspace kaydedildi. Core yeniden başlayınca Herdr yapıyı korudu, `rehydrate` üç komutu doğru terminallere yazdı. Space `dcc workspace close` ile kapatılıp core yeniden başlatıldıktan sonra `restore` aynı yapıyı (sekme adları, klasörler, pane sırası) ve komutları kurdu. Bu çalıştırmada `space.close` dört terminalin dördünü de açık bıraktı, dcc hepsini sonlandırdı (Faz 0'da hepsi kapanmıştı).
    2. Aynı adlı space açıkken `restore` çıkış kodu 1 ve `conflict`, kayıtsız ad çıkış kodu 3 ve `not_found` verdi.
    3. `ensure-layout --verify` DCC layout'unu üretti ve `herdr-app status` hata göstermedi. Sürüm veya girdi değişince yeniden üretme birim ve entegrasyon testleriyle doğrulandı, gerçek Herdr güncellemesiyle denenmedi.
    4. `xtask secret-leak` sahte sırrı gizli prompt üzerinden (pty) kaydetti. 1242 dosya, süreç argümanları ve ekranlarda sızıntı yok (pozitif kontrol bulundu), sır silindi. `secret get` yok (CLI testi).
    5. Mod üzerinden başlatılan 40 sn'lik job 41,4 sn sürdü, pencere 10 ile 15 ms'de yanıt verdi, 39 zamanlayıcı tıkı çalıştı, en yavaş durum sorgusu 32 ms. `sweep` çalışan job'a dokunmadı.
    6. `dev-down` uygulama, iki çekirdek ve holder (4 süreç) PID ile kapalı doğrulandı, 8 terminal sonlandırıldı.
  - Güvenlik taraması (otomatik) 5 bulgu getirdi, hepsi düzeltildi. `rehydrate` yalnızca ada göre ve yinelenen adı reddederek çalışıyor, klasörü değişen veya değiştirilen pane'lere komut yazılmıyor, layout adları `[A-Za-z0-9_-]` ile sınırlı, `dev-down` yalnızca pencere canlıyken PID sorguluyor ve kapatmayı hata durumunda da sürdürüyor.
- Açık sorun.
  - Modun Restore ve Rehydrate düğmelerine tıklanmadı. Yalnızca ekran görüntüsünde listenin ve düğmelerin göründüğü doğrulandı. Düğme eylemi (`exec` ile aynı komutlar) CLI'dan denendi.
  - Özel bölme oranları geri yüklenmedi (yalnızca varsayılan oranlar). Üç ve daha fazla pane'li iç içe ağaç gerçek Herdr'da yalnızca `code` şablonuyla (üç pane) ve 4 pane'li anlık kayıtla denendi.
  - `exists` keyring v1 API'sinde değeri okumadan soramıyor, değeri belleğe alıp atıyor.
  - `secret-leak` BSD `script` kullanıyor (yalnızca macOS), `process_alive` yalnızca Unix. Windows'ta hiçbir şey denenmedi.
  - Modün `config.json` dosyası `dcc` ikilisinin o anki yolunu (debug derlemesi) yazıyor, kurulum yolu Faz sonrası netleşecek.
  - Katmanlı (`.dev`) araç zinciri kopyası spike'tan salt okunur kopyalandı, spike klasörü değişmedi.

## Faz 2, agent paneli ve oturum kaydı (2026-11-12, branch `phase-2-agents`)

- Ne yapıldı.
  - `agent` modülleri (durum eşleme, adapter, hook ve statusline günlükleri, `exec`, `launch`, kayıt, sözleşme testi). SQLite göçü v2 (`agent_runs`, `agent_sessions`, `agent_events`). `dcc-fixture-agent` ve `dcc-agents` modu (salt okunur panel, 2 sn yenileme).
  - Hook günlüğü `DCC_RUN_ID` ile doğrudan SQLite'a yazıyor, snapshot çekirdeği kendisi sorguluyor. `agent interrupt` ve `agent stop` yok, Stopped yalnızca dışarıdan öldürülen süreç demek.
  - Güvenlik sertleştirmesi. Salt okunur git çağrılarında fsmonitor kapalı. `git status` deponun clean filtrelerini boşaltıyor ve alt modülleri atlıyor (pozitif kontrol testleriyle). Hook alanları beyaz listeli, kimlikler doğrulanıyor, worktree silme `--confirm` istiyor.
- Test sonucu.
  - `cargo test` 104 birim, 7 agent CLI, 8 CLI, 3 xtask testi geçti. `cargo clippy --all-targets` temiz.
  - Canlı senaryolar (izole Herdr).
    1. Kabul 1 geçti. Üç worktree agent'ı üç ayrı branch ve klasörle ayrı satırlar olarak kaydedildi (panel verisi `dcc agent snapshot` ile okundu).
    2. Kabul 3 geçti. Çıkış kodu 0 Completed (`exited_ok`), 3 Failed (`exit_nonzero`), `hc kill` Stopped (`killed`).
    3. Kabul 4 geçti. Gerçek Claude Code `hc run` ile başlatıldı, ilk oturum kimliği argv ile aynı geldi. Hook'lara `DCC_RUN_ID` ulaştı (SessionStart, UserPromptSubmit, Stop, SessionEnd). `/clear` sonrası aynı çalıştırmaya bağlı ikinci oturum açıldı (`session_count` 2). Kullanıcı güven ve izin sorularını kendisi yanıtladı. Olaylarda yalnızca olay adları, araç adı ve bildirim türü var.
    4. Kabul 6 geçti. Sözleşme testi ok fixture ile geçti, broken fixture ile `status_reliable` false oldu, sonraki ok ile düzeldi.
    5. Kabul 7 geçti. İki agent aynı klasörde `shared_folder` uyarısı verdi, biri durunca uyarı kalktı.
    6. Kabul 5 birim ve entegrasyon testleriyle geçti (hook adlar dışında alan yazmıyor, kötü bayrakta sıfır çıkış, ortalama yaklaşık 5 ms).
- Açık sorun.
  - Kabul 2 ölçülmedi. Panel 2 sn aralıkla sorguluyor ve snapshot 48 ms sürüyor, yani medyan hedefi tasarım gereği tutuyor ama gerçek gecikme ölçülmedi.
  - Panelin görseli bu fazda doğrulanamadı. Herdr penceresi ekran görüntüsüne gelmedi ve pencereyi öne almak için erişilebilirlik izni gerekiyordu, verilmedi. Boş durum görüntüsü (`.dev/p2-empty.png`) yalnızca ilk kurulumda alındı.
  - Güven sorusu bekleyen agent'ın Unknown notu gerçek Claude ile ayrıca denenmedi.
  - Starting shell pane'inin kapatılması canlı doğrulandı mı sorusu yalnızca agent'ların tek pane ile açılmasından dolaylı görüldü, ayrı test yok.
  - Gerçek Claude testi kullanıcı hesabıyla yapıldı, kota alanına bakılmadı (Faz 6).

## Faz 3, SSH (2026-11-12, branch `phase-3-ssh`)

- Ne yapıldı.
  - `ssh` modülleri (model, db, tarayıcı, askpass, işlemler). Göç v3 (`ssh_groups`, `ssh_profiles`, `port_forwards`, `saved_commands`). Parola veya passphrase SQLite'ta yok, Keychain'de `ssh.<profil>` adıyla duruyor.
  - Sır yolu. `dcc ssh run` (ssh'i başlatan süreç) Keychain'den değeri bir kez okuyor ve özel bir unix soketinden sunuyor (0700 klasör, 0600 soket). ssh `SSH_ASKPASS` ile küçük bir sarmalayıcıyı çalıştırıyor, o da `dcc ssh askpass` ile sokete soruyor. Soket yalnızca o çalıştırmaya özel rastgele nonce'u bilene, profilin beklediği türde bir istem için ve en fazla üç kez cevap veriyor. Host key sorusu her zaman reddediliyor. Değer argv'ye, dosyaya, log'a ve ekrana gitmiyor.
  - Bağlantı `hc run` ile `ssh` space'inde kendi sekmesinde açılıyor (`herdr/tabs.rs` agent başlatma ile ortak). Yönlendirmeler (local, remote, dynamic) etiketli `hc run --bg` terminali, durum çekirdekten (`running`, `stopped`, `failed`, `idle`). Bind adresi yalnızca loopback.
  - Kayıtlı komutlar tek satır, kayıtta sır deseni taraması (yalnızca tür söyleniyor, metin değil). Çalıştırma `hc send` ile, Herdr durumu blocked olan terminale `--force` olmadan yazılmıyor.
  - Gizli giriş terminalde. `dcc ssh profile secret` yalnızca terminalde çalışıyor. Mod için `secret-tab` komutu gizli istemi ayrı bir sekmede açıyor, mod değeri görmüyor.
  - `dcc-ssh` modu (profil listesi, Connect, Set secret, yönlendirme Start ve Stop, komut Run) ve `dcc ssh overview`. Layout tarifine eklendi.
  - Güvenlik sertleştirmesi (otomatik taramadan). Çalışan ssh sistem yolundan seçiliyor, `DCC_SSH_PROGRAM` yalnızca debug derlemede ve mutlak yolla geçerli. Sır gerektiren profillerde `PermitLocalCommand=no` ve `KnownHostsCommand=none`.
- Test sonucu.
  - `cargo test` 125 birim, 7 agent CLI, 8 CLI, 4 ssh CLI, 3 xtask testi geçti. `cargo clippy --all-targets` temiz.
  - Canlı senaryo (izole Herdr, yalnızca `127.0.0.1:2222` üzerinde kendi başlattığım sshd, `~/.ssh` kullanılmadı).
    1. Kabul 1 geçti. Passphrase pty üzerinden gizli istemle Keychain'e girildi, değer isteme yankılanmadı. Sızıntı taraması (`xtask leak-scan`, 1260 dosya, süreç argümanları ve terminal ekranları) temiz, ayrıca CLI testi `secret get` olmadığını gösteriyor. Tarama bağlantılar ve yönlendirmeler çalışırken yapıldı.
    2. Kabul 2 geçti. Anahtarlı profil ve passphrase'li profil sekmede açıldı, ön plandaki program dcc, kayıtlı komut uzak kabukta çalıştı (çıktı ekranda görüldü). Passphrase askpass ile verildi.
    3. Kabul 3 geçti. Local, remote ve dynamic yönlendirme (ve passphrase'li profille local) açıkken HTTP isteği hedefe ulaştı, durum `running` görüldü, `stop` sonrası istek düştü ve durum `stopped` oldu. İkinci start reddedildi. Port dolu olan yönlendirme `failed` (çıkış 255) gösterdi.
    4. Kabul 4 geçti. Kayıtlı komut seçilen terminalde çalıştı. Blocked terminal reddi sahte Herdr ile birim testte, canlı denenmedi.
    5. Kabul 5. Gerçek uzak makine, parolayla gerçek giriş, `ssh-agent` ve SFTP gezgini bu fazda yapılmadı ve kapsam dışı.
  - Temizlik. Test profilleri `--confirm` ile silindi (Keychain kaydı `ssh.pp1` silindi, yok), sshd kendi PID'siyle kapatıldı, `dev-down` uygulama ve çekirdeği PID ile kapalı doğruladı.
- Açık sorun.
  - Paneli gözle doğrulamadım (pencere ekran görüntüsüne gelmiyor, erişilebilirlik izni verilmedi). Modun `check` denetimi geçti ve layout `herdr-app status` hatasız, `dcc ssh overview` çıktısı elle denendi, düğmelere tıklanmadı.
  - Parola (password) profili gerçek sshd ile denenmedi. Kök yetkisiz sshd parola doğrulayamıyor. Aynı yol (askpass sunucusu ve sarmalayıcı) passphrase ile canlı ve parola istemi için birim ve entegrasyon testinde denendi.
  - Host key `strict` ile bilinmeyen host ve askpass bileşimi yalnızca birim testte (askpass reddi), canlı denenmedi.
  - Askpass sunucusu çağıranı süreç kimliğiyle denetlemiyor (güvenli Rust ile olmuyor). ssh'in kendi başlattığı bir program (kullanıcı config'indeki ProxyCommand) nonce'u okuyabilir, sınırı üç cevap, yalnızca parola türü istem ve ssh çalıştığı süre. `askpass.rs` başında yazılı.
  - Profil güncelleme komutu yok (silip yeniden ekleniyor). Gizli giriş, `-F` dosyası ve `KnownHostsCommand` OpenSSH 8.5 ve üstü gerektiriyor.
  - Linux ve Windows'ta denenmedi, askpass Unix soketi ve `/dev/urandom` kullanıyor.

## Faz 4, Git ve kod sekmesi (2026-11-12, branch `phase-4-git`)

- Ne yapıldı.
  - `dcc-code` modu. Küçük bir panel (odaktaki klasör, branch, worktree türü, değişen dosya sayısı, iki düğme) ve iki komut, `dcc-code.lazygit-popup` ve `dcc-code.code-tab`. Herdr bir modun komutlarını yalnızca mod yüklüyken tanıdığı için mod layout'ta panel olarak duruyor (`place` none yetmedi, komut "unknown" döndü).
  - Popup. Odaktaki terminalin klasöründe `hc run --bg` ile lazygit başlatılıyor ve pencere overlay'inde gösteriliyor. Terminal adı her seferinde benzersiz (aynı ad ikinci popup'ı engelliyordu, canlı testte görüldü).
  - Kod sekmesi. `code` şablonu (Faz 1'den) odaktaki klasörle `dcc workspace template apply` üzerinden açılıyor.
  - `dcc git tools` neovim ve lazygit'in kurulu olup olmadığını söylüyor, eksikse kurulum komutunu gösteriyor. DCC bir şey kurmuyor.
  - Worktree komutları (`add`, `list`, `remove --confirm`) ve git bilgisi (`dcc git info`) Faz 2'den geliyor, aynı kod agent paneli ve `dcc-code` panelinde ortak. Bu fazda bunlar için CLI entegrasyon testi eklendi.
  - DCC içinde diff, stage veya commit arayüzü yok, hepsi lazygit'te.
- Test sonucu.
  - `cargo test` 126 birim, 7 agent CLI, 8 CLI, 4 ssh CLI, 2 git CLI, 3 xtask testi geçti. `cargo clippy --all-targets` temiz.
  - Canlı senaryo (izole Herdr).
    1. Kabul 1 geçti. Mod komutuyla kod sekmesi açıldı (üç pane, `nvim`, `lazygit`, shell), space kapatılıp (üç terminal sonlandırıldı) `restore` edildi, aynı sekme ve pane düzeni geldi, startup komutları yazıldı. Bölme oranları yaklaşık (Faz 1'deki sınır).
    2. Kabul 2 geçti. `t2-repo` space'i odaktayken popup lazygit'i `main` branch'iyle, `t2-repo-b` odaktayken `feature/popup-b` ile açtı (terminal klasörü ve lazygit ekranı doğrulandı).
    3. Kabul 3 geçti. Worktree oluşturma, listeleme ve linked worktree bilgisi CLI testinde, silme `--confirm` olmadan reddedildi.
    4. Kabul 4 geçti (mod ve Rust kodunda diff, stage veya commit yok).
  - `dev-down` uygulama ve çekirdeği PID ile kapalı doğruladı.
- Açık sorun.
  - "Bir tuşla" kısmı. Komutlar `herdr-app do` ile çalışıyor ama bir tuşa bağlamadım. Bunun için Herdr'ın tuş ayarını yazmak gerekir (yapılandırma yazma kuralı, yedek ve fark önizlemesi ister), kullanıcıya bırakıldı.
  - Overlay'in ekranda görüldüğü gözle doğrulanamadı (pencere ekran görüntüsüne gelmiyor, erişilebilirlik izni yok). Doğrulama terminal, klasör ve lazygit ekran metniyle yapıldı.
  - Düğmelere tıklanmadı, komutlar `herdr-app do` ile çağrıldı. Düğme eylemi aynı fonksiyonları çağırıyor.
  - Layout'ta artık dört panel var (340, 360, 360, 240 px). Küçük ekranda terminale az yer kalır, genişlikler ayarlanmalı veya paneller ayrı layout'lara bölünmeli (ürün kararı, bekliyor).
  - Neovim ve lazygit'in gerçek kullanımı (düzenleme, commit, rebase) denenmedi.

### Faz 4 eki, paneller tab çubuğunda (2026-11-12, branch `phase-4-git`)

Kullanıcı dört panelin terminali kapattığını söyledi ve panellerin sağ üstte `+` işaretinin sağında sekme gibi açılıp kapanmasını istedi. Bu iş Faz 4 branch'ine commit'lendi (kullanıcı "sonraki faza ekle" dedi, ama Faz 5 branch'i henüz açılmamıştı, taşımak geçmişi yeniden yazmak olurdu).

- Ne yapıldı.
  - DCC panelleri artık Herdr'ın kendi yan paneline (`panel-body` yığını, Files ve Changes ile aynı yer) giriyor. Aynı anda tek panel açık, kapalıyken terminal tam genişlikte. Düğmeler (Workspaces, Agents, SSH, Code) tab çubuğunda `+` ve Files, Changes düğmelerinin sağında, açık olan vurgulu, ikinci tık kapatıyor.
  - Tab çubuğu Herdr'ın salt okunur `tabs` modundan türetiliyor, kaynağı repoda yok. `dcc herdr ensure-tabs` kurulu `tabs` modunu okuyor, üç tam eşleşmeli metin değişikliği yapıp `dcc-tabs` olarak yazıyor. Bir eşleşme tutmazsa (Herdr sürümü değişirse) hiçbir şey yazılmıyor ve komut hata veriyor.
  - Layout tarifine `set` eklendi (var olan düğümün alanını değiştirir). Tarif `tabs` bloğunu `dcc-tabs` yapıyor ve DCC bloklarını `panel-body` içine koyuyor. Kurulum sırası `ensure-mods`, `ensure-tabs`, `ensure-layout`.
- Test sonucu.
  - `cargo test` 130 birim, 7 agent CLI, 8 CLI, 2 git CLI, 4 ssh CLI, 3 xtask testi geçti. `cargo clippy --all-targets` temiz.
  - Canlı doğrulama `herdr-app shot` ile ekran görüntüsü alınarak yapıldı (Agents, SSH ve Code panelleri açıldı, düğme vurgulandı, kapatınca terminal tam genişliğe döndü). `layout.select` ve `layout.show` komutları düğmelerin çağırdığıyla aynı.
  - Bu görüntüler Faz 2 ve 3 için açık kalan "panel gözle doğrulanmadı" maddesini de kapatıyor (Agents ve SSH panelleri görüldü). Workspaces paneli görüntüsü alındı ama incelenmedi. Düğmelere gerçek fare tıklaması yapılmadı.
- Açık sorun.
  - `herdr-app shot` Herdr'ın son kare görüntüsünü veriyor, `screencapture` yerine bunu kullanmak gerekiyor.
  - Bir panel açıkken diğer panel görünmüyor, yan yana iki DCC paneli gösterilemez. Panel genişliği Herdr'ın `panel` düğümünden geliyor (min 280).
