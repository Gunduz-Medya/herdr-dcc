# herdr-dcc

Developer Command Center (DCC), Herdr masaüstü uygulamasının içine takılan bir eklenti. Birden çok AI agent'ı, SSH bağlantılarını, Git işlerini ve AI ayarlarını Herdr'ın tek penceresinde toplar. Kendi terminalini veya kendi senkron motorunu yazmaz, Herdr, lazygit, neovim ve Skillshare'i birbirine bağlar.

Durum. Altı ürün fazı kodlandı ve test edildi (aşağıya bakın), ama henüz gerçek bir Herdr kurulumuna yüklenmedi. Tüm denemeler yalıtılmış bir Herdr kopyasında yapıldı. Hedef platform macOS Apple Silicon, Windows bu aşamada kapsam dışı.

## Parçalar

- `crates/dcc` komut satırı programı ve kütüphane (SQLite kaydı, Herdr çağrıları, agent kaydı, SSH, Git, AI konfigürasyonu, kullanım).
- `crates/dcc-fixture-agent` testler için sahte agent.
- `crates/xtask` geliştirme ortamı yardımcıları.
- `mods/` Herdr içinde çalışan paneller (workspaces, agents, ssh, code, config, usage).
- `layouts/` ve `assets/` Herdr'ın kendi düzenine eklenen DCC düzeni ve simgeler.
- `docs/` kapasite envanteri, ürün gereksinimleri, mimari, karşılaştırma, yol haritası, doğrulama testleri ve her fazın kaydı (`PROGRESS.md`).

## Fazlar

1. Çekirdek (workspace kaydı, Keychain, job deseni, Herdr katmanı)
2. Agent paneli ve oturum kaydı
3. SSH (profil, port yönlendirme, kayıtlı komutlar)
4. Git ve kod sekmesi (lazygit, neovim)
5. AI konfigürasyonu (envanter, Skillshare önizleme ve senkron)
6. Kullanım ve kota (yalnızca resmi kaynaktan, tahmin yok)

Her fazın kabul ölçütleri ve sonucu `docs/05-roadmap.md` ile `docs/PROGRESS.md` içinde.

## Derleme ve test

Rust'ın güncel kararlı sürümü gerekir.

```sh
cargo build
cargo test --workspace
cargo clippy --all-targets
```

Testler gerçek Herdr'a veya gerçek `~/.claude` klasörüne dokunmaz. Bazı testler `skillshare` kuruluysa çalışır, yoksa atlanır.

## Kullanım

`dcc` komutu çıktıyı JSON olarak verir. Bir veri klasörü gerekir (`--data-dir` veya `DCC_DATA_DIR`). Yardım için `dcc --help`.

Herdr'a kurulum (üç adım, sırayla) şu komutlarla yapılır, ama bunlar gerçek Herdr ayar klasörüne yazar, önce `docs/PROGRESS.md` içindeki notları okuyun.

```sh
dcc herdr ensure-mods
dcc herdr ensure-tabs
dcc herdr ensure-layout --recipe layouts/dcc.recipe.json
```

## Güvenlik ilkeleri

- Sırlar yalnızca Keychain'de durur, argv'ye, dosyaya, log'a veya ekrana yazılmaz. `secret get` yoktur.
- DCC `~/.claude` ayarlarına yazmaz. Yapılandırmaya yazan her eylem yedek, önizleme ve ayrı silme onayı ister.
- Kullanım ve kota rakamı hiçbir zaman hesaplanmaz veya tahmin edilmez. Resmi kaynak yoksa "Unavailable" yazar.

## Lisans

Henüz bir lisans seçilmedi, tüm hakları saklıdır.
