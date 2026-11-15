# Taskbar (Rust) — Mimari Tasarımı

Durum: **onaylandı** · Tarih: 2026-11-15 · Repo: `https://github.com/merterdem13/Taskbar`

## Amaç ve neden
Windows için, mevcut battery widget'ı ile uyumlu/gömülü, görsel olarak çekici, **çok az RAM ve sıfıra yakın CPU** kullanan kendi taskbar'ımız. Kullanıcı konumu (üst/alt/sol/sağ), stili ve modülleri değiştirebilmeli; bunun için kurulumda ve gerektiğinde açılan bir ayarlar uygulaması ve kullanıcı özelleştirmelerini kaybetmeden çalışan bir GitHub Releases güncelleme sistemi olmalı.

## Hedefler
- Bar boşta CPU ≈ 0, RAM hedefi 10–20 MB (prototip: 60 MB çalışma kümesi / 45 MB private, bkz. Açık işler).
- Dört kenarı destekleyen appbar (Windows'un kendi taskbar'ına dokunmaz).
- V1 kapsamı: **sadece widget şeridi** (pil, saat, ses, sistem bilgisi). Pencere/uygulama butonları yok.
- Ayarlar uygulaması ayrı süreç; **canlı önizleme** destekli.
- **GitHub Releases tabanlı güncelleme denetleyicisi:** Bar sürecine ağ/HTTP yükü bindirmeden ayrı süreç üzerinden güncelleme kontrolü ve tetiklemesi; güncelleme sırasında kullanıcı ayarlarının (`%APPDATA%\Taskbar\config.toml`) %100 korunması.
- Mevcut battery widget'ın okuyucuları (Razer/SteelSeries HID, Bluetooth SetupAPI) modül olarak taşınır (`TaskbarBatteryWidget` reposuna dokunulmaz, bu proje bağımsız `merterdem13/Taskbar` reposunda yaşar).

## Hedef olmayanlar / anti-hedefler
- `explorer.exe` hooklamak veya Windows taskbar'ını yamalamak (kırılgan).
- Bar sürecine ağır UI çerçevesi (WebView, wgpu) eklemek: bellek hedefini bozar.
- Polling tabanlı sürekli döngüler.
- V1'de pencere listesi, sistem tepsisi barındırma, tam tema mağazası.

## Kısıtlar
- Yalnız Windows 10/11. Acrylic/Mica için Windows 11 önceliği, Windows 10'da düz/yarı saydam yedek.
- Çoklu monitör ve farklı DPI desteklenecek şekilde tasarlanır (v1'de birincil ekran yeterli).

## Yaklaşımlar (değerlendirilen)
| Yaklaşım | Bellek | Not | Karar |
|---|---|---|---|
| A. `windows-rs` + Direct2D/DirectComposition/DirectWrite | 62.88 MB WS / 47.59 MB Private | Tam kontrol; D2D/DirectWrite yığını RAM yiyor | Yedek (`render-d2d` feature) |
| A'. `windows-rs` + `tiny-skia` CPU çizim + `fontdue` + DWM BGRA sunum | **10.81 MB WS / 3.55 MB Private** | DWM Acrylic/Mica + anti-aliased pill + net metin/sembol, 0 ms boşta CPU | **Seçildi (`render-skia` varsayılan)** |
| B. Slint | ~15–25 MB (tahmin) | Hızlı geliştirme | Yedek |
| E. Tauri/WebView2 (bar için) | 50–100+ MB | Hedefe ters | **Elendi** (bar için) |

Render yığını bir **render soyutlaması** arkasında tutulur. **Kullanıcı kararı: görsellik çok önemli, dengeli yaklaşım.** Seçim ölçütü (benim önerim, kullanıcı "dengeli" dedi):
- Görsel kalite **sabit taban**: acrylic/mica arka plan, anti-aliased yuvarlak köşe, ClearType/gri tonlu net metin, akıcı animasyon (DWM tarafında). Bu tabandan ödün verilmez.
- Bu tabanı sağlayan adaylar arasında **en az belleği kullanan** seçilir. Hedef 10–20 MB; 30 MB'ı geçerse gerekçelendirilir.
- Karar, aynı görsel tasarımı A ve A' ile uygulayan küçük bir karşılaştırma prototipinin ölçümüyle verilir (ilk uygulama görevi).

## Mimari kararlar (kullanıcıyla konuşulanlar)
1. **Bar ve ayarlar/güncelleyici ayrı süreç.** Ayarlar ve güncelleme denetleyicisi yalnız ilk kurulumda, kullanıcı istediğinde veya zamanlanmış kontrol anında çalışır, sürekli açık kalmaz; bu yüzden `bar` (`taskbar.exe`) sürecine ağ/HTTP/TLS veya WebView bellek maliyeti yüklemez.
2. **Cargo workspace:**
   ```
   crates/core      # config şeması + sürüm göçü (migration), tema, yerleşim, modül trait'i, sürüm/manifest karşılaştırma (platformdan bağımsız, test edilebilir)
   crates/bar       # taskbar.exe: Win32 appbar, çizim, modüller, sağ-tık menüsü (Ayarlar / Güncellemeleri Denetle / Çıkış), temiz kapanış sinyali
   crates/updater   # taskbar-updater.exe (veya settings alt komutu): GitHub Releases denetleyicisi, indirici, bar'ı temiz kapatıp ikilileri değiştirip yeniden başlatıcı
   crates/settings  # (sonra) Tauri uygulaması
   ```
3. **Yapılandırma ve Güncellemelerde Veri Korunumu:**
   - Kullanıcı ayarları kurulum dizininde değil, **`%APPDATA%\Taskbar\config.toml`** altında tutulur.
   - Güncelleyici yalnızca uygulama ikililerini (`taskbar.exe`, `taskbar-settings.exe`, `taskbar-updater.exe`) değiştirir; `%APPDATA%\Taskbar\config.toml` dosyasına **asla** dokunmaz veya üzerine varsayılan dosya yazmaz.
   - Şema sürümlüdür (`version = 1`). Her alan `#[serde(default)]` kullanır ve `Config::migrate(raw_toml)` fonksiyonu eski sürüm dosyaları yeni sürüme taşırken kullanıcının özelleştirdiği alanları (ör. `edge = "right"`, özel renkler, kalınlık, modül sırası) birebir korur, yalnızca yeni eklenen alanlara varsayılan değer atar.
4. **Canlı güncelleme (Config):**
   - Kalıcı değişiklik: bar `ReadDirectoryChangesW` ile `%APPDATA%\Taskbar` dizinini izler (olay tabanlı).
   - **Canlı önizleme:** ayarlar uygulaması bar'a **named pipe** üzerinden geçici config gönderir; "Kaydet" ile dosyaya yazılır, "İptal" ile eski haline döner. Ayarlar kapanınca/çökünce bar kaydedilmiş config'e döner.
5. **Yerleşim motoru ve Dikey Kenar (`Left`/`Right`) Davranışı:**
   - Başlangıçta doğrusal yerleşim, karmaşık esnek kutu gerektiğinde `taffy` (flexbox).
   - Kenardan bağımsız: üst/alt yatay, sol/sağ dikey akış.
   - **Dikey kenar kuralı (kullanıcı onayladı):** Sol ve sağ kenarlarda bar genişliği `thickness` (varsayılan 40 px) ile sınırlı olduğundan modüller `text_for_edge(edge)` ile **iki satırlı/kompakt formatta** (ör. saat `09\n05`, pil `82%\n⚡`) üretilir ve pill genişliği bar kalınlığına (`thickness - padding`) sığdırılır (taşma nedeniyle modül düşmesi önlenir).
6. **Modül trait'i:** her widget kenara göre (`Edge`) metnini/boyutunu bildirir, çizilir, olay gelince "güncellendim" sinyali verir. Veri kaynakları olay tabanlıdır (ör. `WM_POWERBROADCAST`, dakika sınırına hizalanmış timer).
7. **Tema = veri:** renk, yazı tipi, boyutlar kodda gömülü değil.
8. **Ayarlar uygulaması teknolojisi (kullanıcı onayladı):** Tauri; bar'dan ayrı süreç olduğu için WebView maliyeti kabul edilebilir.
9. **GitHub Releases Güncelleme Mimarisi (kullanıcı Yaklaşım A'yı seçti):**
   - **Değerlendirilen yaklaşımlar:**
     - *Yaklaşım A (Seçilen):* Güncelleme kontrolü ve indirme işlemi ayrı süreçte (`taskbar-updater` / `settings`) çalışır; `bar` içinde HTTP/TLS yığını bulunmaz.
     - *Yaklaşım B (Elendi):* `bar` içinde WinHTTP ile arka plan kontrolü — her zaman açık olan sürece gereksiz ağ/durum yükü ekler.
     - *Yaklaşım C (Yedek):* Tauri yerleşik NSIS güncelleyicisi.
   - **Çalışma akışı:**
     1. Kullanıcı bar sağ-tık menüsünden *"Güncellemeleri Denetle"* seçtiğinde veya Ayarlar uygulamasını açtığında `taskbar-updater` süreci `https://api.github.com/repos/merterdem13/Taskbar/releases/latest` uç noktasını sorgular ve mevcut `CARGO_PKG_VERSION` ile karşılaştırır (`core::update::is_newer_version`).
     2. Yeni sürüm varsa indirilen paket (`%TEMP%` altına) doğrulanır.
     3. Güncelleyici, çalışan `taskbar.exe` penceresine temiz kapanış sinyali (`WM_CLOSE` veya Named Pipe `ShutdownForUpdate`) gönderir.
     4. `taskbar.exe` `SHAppBarMessage(ABM_REMOVE)` çağırarak masaüstü çalışma alanını serbest bırakır ve kapanır.
     5. Güncelleyici kurulum dizinindeki `.exe` dosyalarını günceller (`%APPDATA%\Taskbar\config.toml` dosyasına dokunmaz) ve yeni `taskbar.exe`'yi başlatır.
     6. Yeni `taskbar.exe` açılırken `%APPDATA%\Taskbar\config.toml` dosyasını okur ve kullanıcının bıraktığı kenarda (ör. sağ kenar) ve temada kesintisiz çalışmaya devam eder.

## Mevcut battery widget
`C:\Users\Mert\.gemini\antigravity\scratch\TaskbarBatteryWidget` (C#/WPF, son hali değil). Okuyucu mantığı (`BatteryMonitorService`, `NativeBluetoothBatteryReader`, `RazerLogReader`) referans alınıp Rust modülüne taşınacak; **taşıma V1 sonrası ilk modül işi**.

## Builder'a (uygulayıcıya) bırakılanlar
- Named pipe protokol ayrıntıları (mesaj formatı, sürümleme).
- Şema alan adları ve varsayılanlar.
- Render yığınının ölçüme göre seçimi (A / A' / B).
- Çoklu monitör davranışı.

## Açık işler
- **Tamamlandı (Render Kararı):** Direct2D (`render-d2d`: 62.88 MB Working Set / 47.59 MB Private Bytes) ile `tiny-skia` + `fontdue` (`render-skia`: **10.81 MB Working Set / 3.55 MB Private Bytes**, 0 ms boşta CPU) ölçüldü (`docs/superpowers/specs/render-measurements.md`). Görsel tabanı koruyarak 10–20 MB bellek hedefini yakalayan `render-skia` varsayılan backend olarak seçildi.
- Prototip görsel olarak kullanıcı tarafından henüz doğrulanmadı.
