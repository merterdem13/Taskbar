# Ayarlar Penceresi Entegre Üst Çubuk ve Başlık Sadeleştirme

**Tarih:** 2026-11-16
**Kapsam:** `crates/settings` (`taskbar-settings.exe`)

## Amaç ve Neden

Mevcut Ayarlar penceresi standart `WS_OVERLAPPEDWINDOW` başlık çubuğunu kullandığı için `#070708` uygulama arka planının üzerinde ayrı bir koyu gri şerit oluşturuyor ve pencere içi başlıkla birlikte `"Taskbar Ayarları"` ifadesini iki kez gösteriyor. Amaç, Windows başlık çubuğunu uygulamanın `#070708` yüzeyiyle tek parça haline getirmek ve başlığı şimdilik `"Taskbar"` olarak sadeleştirmektir.

## Hedefler, Kapsam Dışı ve Anti-Hedefler

- **Hedefler:**
  - Standart Windows üst şeridini kaldırıp pencere kontrollerini (`Küçült`, `Büyüt / Geri Yükle`, `Kapat`) doğrudan uygulama yüzeyinin sağ üstüne entegre etmek.
  - Başlık çubuğu sürükleme, çift tıklamayla büyütme/geri yükleme ve tüm kenarlardan/köşelerden pencere boyutlandırma davranışlarını korumak.
  - Windows 11 gölge ve yuvarlatılmış pencere köşelerini korumak.
  - Pencere başlığını (`main.rs`) ve uygulama içi ana başlığı (`ui.rs`) `"Taskbar"` olarak güncellemek.
- **Kapsam Dışı:** Sekme içeriklerinin yeniden tasarımı, `crates/bar` veya `crates/core` değişiklikleri, Windows 11 Snap Layouts menüsü.
- **Anti-Hedefler:** Üstte ikinci bir şerit bırakmak veya pencere kenarlık/gölge/maksimize sınırlarını bozan salt `WS_POPUP` kullanmak.

## Kısıtlar

- `WS_OVERLAPPEDWINDOW` stili korunup `WM_NCCALCSIZE` üzerinden non-client alan sıfırlanmalı; `DwmExtendFrameIntoClientArea` ile pencere gölgesi/kenarlığı ve maksimize durumda (`IsZoomed`) monitör çalışma alanı sınırı doğru hesaplanmalıdır.
- Pencere kontrolleri `WM_NCHITTEST` yerine doğrudan client alanında veya `HTCLIENT` üzerinden işlenmeli; sürüklenebilir başlık ve `8 px` kenar tutma bölgeleri canlı durum hapı ile pencere butonlarını engellememelidir.

## Değerlendirilen Yaklaşımlar

1. **Özel Entegre Üst Çubuk (`WM_NCCALCSIZE` + `WM_NCHITTEST` + Direct2D Kontroller) — Seçilen:**
   Ayrı OS başlık şeridini kaldırır; pencere kontrollerini ve `"Taskbar"` başlığını tek bir `#070708` üst satırda birleştirir. Tam bütünlük sağlar.
2. **DWM Başlık Rengi Eşleştirme (`DWMWA_CAPTION_COLOR`):**
   Native Windows butonlarını ve Snap Layouts'u korur ancak üstte ayrı bir başlık satırı bırakır; tek parça entegrasyon hedefi nedeniyle elendi.

## Tasarım Özeti

- **Başlık Metni:** [main.rs](file:///c:/Users/Mert/Documents/Taskbar/crates/settings/src/main.rs#L437) pencere adı ve [ui.rs](file:///c:/Users/Mert/Documents/Taskbar/crates/settings/src/ui.rs#L1122) ana başlığı `"Taskbar"` olur (`ui.rs` birim testi buna göre güncellenir).
- **Üst Satır Yerleşimi:**
  - Solda `"Taskbar"` başlığı ve altında mevcut alt başlık.
  - Sağ üstte üç pencere butonu (`—`, `□`, `×`), hemen solunda `● Canlı Önizleme` / `○ Çevrimdışı` durum hapı.
  - Pencere butonları `SettingsPainter` üzerine eklenecek vektör çizgi yardımcısıyla Direct2D'de keskin biçimde çizilir; küçült/büyüt butonlarında yumuşak koyu hover, kapatma butonunda kırmızı (`#E81123`) hover uygulanır.
  - Giriş (splash) ekranında da üst sürükleme alanı ve pencere butonları kullanılabilir kalır.
- **Win32 Pencere Entegrasyonu:**
  - `WM_NCCALCSIZE`: `wParam != 0` iken client rect korunur (maksimize durumda OS taşması monitör `rcWork` veya çerçeve kalınlığı ile kırpılır).
  - `WM_NCHITTEST`: `8 px` dış kenarlar/köşeler boyutlandırma (`HTLEFT`, `HTTOPLEFT`, vb.), üst başlık şeridi (durum hapı ve butonlar hariç) `HTCAPTION`, geri kalanı `HTCLIENT` döner.
  - Pencere butonları `WM_LBUTTONDOWN` / `HitRegion` üzerinden `ShowWindow(..., SW_MINIMIZE)`, `SW_MAXIMIZE`/`SW_RESTORE` ve `WM_CLOSE` tetikler.

## Uygulamacıya Bırakılanlar

- Pencere butonlarının kesin boyutları (`42x32 px` vb.), ikon çizgi kalınlığı ve `ui.rs` yerleşim boşluklarının 520×460 minimum boyutta çakışmayacak şekilde ayarlanması.
- İlgili birim testlerinin güncellenmesi ve eklenmesi.
