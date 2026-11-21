//! Small built-in translation table.
//!
//! The app keeps English as the source language. Page titles and common
//! controls have German, French and Turkish translations. Option help falls
//! back to English when a translation is missing.
//!
//! The table is intentionally data, not code. A gettext catalog can replace
//! it later without changing the call sites.

use std::sync::OnceLock;

static LANGUAGE: OnceLock<String> = OnceLock::new();

pub fn language() -> &'static str {
    LANGUAGE.get_or_init(|| {
        let raw = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_else(|_| "en".to_string());
        raw.split(['_', '.', '@'])
            .next()
            .unwrap_or("en")
            .to_string()
    })
}

/// Translate a static UI string.
pub fn tr(text: &'static str) -> &'static str {
    lookup(text).unwrap_or(text)
}

/// Translate a string that comes from data, such as an option label.
pub fn tr_owned(text: &str) -> String {
    lookup(text).unwrap_or(text).to_string()
}

fn lookup(text: &str) -> Option<&'static str> {
    let table = match language() {
        "de" => DE,
        "fr" => FR,
        "tr" => TR,
        _ => return None,
    };
    table
        .iter()
        .find(|(source, _)| *source == text)
        .map(|(_, translated)| *translated)
}

const DE: &[(&str, &str)] = &[
    (
        "This is the system-wide config. Make a personal copy to change it.",
        "Dies ist die systemweite Konfiguration. Erstelle eine persönliche Kopie, um sie zu ändern.",
    ),
    ("Make a personal copy", "Persönliche Kopie erstellen"),
    (
        "Turn on {name} to use this.",
        "Schalte {name} ein, um dies zu nutzen.",
    ),
    ("{n} changes", "{n} Änderungen"),
    ("Undid {n} changes", "{n} Änderungen rückgängig gemacht"),
    ("Redid {n} changes", "{n} Änderungen wiederhergestellt"),
    ("Style", "Stil"),
    ("Input", "Eingabe"),
    ("System", "System"),
    ("Advanced", "Erweitert"),
    ("Pages", "Seiten"),
    (
        "No display positions are available.",
        "Keine Anzeigepositionen verfügbar.",
    ),
    (
        "Drag a display. Edges snap. The change applies when you release.",
        "Ziehe eine Anzeige. Kanten rasten ein. Die Änderung gilt beim Loslassen.",
    ),
    (
        "Release to apply. niri settings asks you to keep the change.",
        "Loslassen zum Anwenden. niri settings fragt, ob du die Änderung behalten willst.",
    ),
    ("Search results", "Suchergebnisse"),
    ("Home", "Start"),
    ("Appearance", "Darstellung"),
    ("Windows and layout", "Fenster und Layout"),
    ("Apps", "Apps"),
    ("Keyboard", "Tastatur"),
    ("Shortcuts", "Tastenkürzel"),
    ("Mouse and touchpad", "Maus und Touchpad"),
    ("Displays", "Anzeigen"),
    ("Workspaces", "Arbeitsbereiche"),
    ("Animations", "Animationen"),
    ("Startup", "Autostart"),
    ("Check my setup", "Einrichtung prüfen"),
    ("All options", "Alle Optionen"),
    ("Config file", "Konfigurationsdatei"),
    ("Search settings", "Einstellungen suchen"),
    ("Undo", "Rückgängig"),
    ("Redo", "Wiederholen"),
    ("History", "Verlauf"),
    ("Preferences", "Einstellungen"),
    (
        "Import from Hyprland or Sway",
        "Aus Hyprland oder Sway importieren",
    ),
    ("Copy as Nix", "Als Nix kopieren"),
    ("About niri settings", "Über niri settings"),
    ("Open", "Öffnen"),
    ("Apply", "Anwenden"),
    ("Add", "Hinzufügen"),
    ("Save", "Speichern"),
    ("Close", "Schließen"),
    ("Cancel", "Abbrechen"),
    ("Revert", "Zurücksetzen"),
    ("Keep changes", "Änderungen behalten"),
    (
        "Reset to the niri default",
        "Auf den niri-Standard zurücksetzen",
    ),
    ("About this setting", "Über diese Einstellung"),
    ("Your setup", "Deine Einrichtung"),
    ("At a glance", "Auf einen Blick"),
    ("Recent changes", "Letzte Änderungen"),
    ("From another compositor", "Von einem anderen Compositor"),
    ("Problems", "Probleme"),
    ("Check", "Prüfen"),
    ("Import", "Importieren"),
    ("Main menu", "Hauptmenü"),
    ("Display on", "Anzeige an"),
];

const FR: &[(&str, &str)] = &[
    (
        "This is the system-wide config. Make a personal copy to change it.",
        "Ceci est la configuration du système. Créez une copie personnelle pour la modifier.",
    ),
    ("Make a personal copy", "Créer une copie personnelle"),
    (
        "Turn on {name} to use this.",
        "Activez {name} pour utiliser ce réglage.",
    ),
    ("{n} changes", "{n} modifications"),
    ("Undid {n} changes", "{n} modifications annulées"),
    ("Redid {n} changes", "{n} modifications rétablies"),
    ("Style", "Style"),
    ("Input", "Saisie"),
    ("System", "Système"),
    ("Advanced", "Avancé"),
    ("Pages", "Pages"),
    (
        "No display positions are available.",
        "Aucune position d'écran n'est disponible.",
    ),
    (
        "Drag a display. Edges snap. The change applies when you release.",
        "Faites glisser un écran. Les bords s'alignent. Le changement s'applique au relâchement.",
    ),
    (
        "Release to apply. niri settings asks you to keep the change.",
        "Relâchez pour appliquer. niri settings demande de conserver le changement.",
    ),
    ("Search results", "Résultats de recherche"),
    ("Home", "Accueil"),
    ("Appearance", "Apparence"),
    ("Windows and layout", "Fenêtres et disposition"),
    ("Apps", "Applications"),
    ("Keyboard", "Clavier"),
    ("Shortcuts", "Raccourcis"),
    ("Mouse and touchpad", "Souris et pavé tactile"),
    ("Displays", "Écrans"),
    ("Workspaces", "Espaces de travail"),
    ("Animations", "Animations"),
    ("Startup", "Démarrage"),
    ("Check my setup", "Vérifier ma configuration"),
    ("All options", "Toutes les options"),
    ("Config file", "Fichier de configuration"),
    ("Search settings", "Rechercher un réglage"),
    ("Undo", "Annuler"),
    ("Redo", "Rétablir"),
    ("History", "Historique"),
    ("Preferences", "Préférences"),
    (
        "Import from Hyprland or Sway",
        "Importer depuis Hyprland ou Sway",
    ),
    ("Copy as Nix", "Copier en Nix"),
    ("About niri settings", "À propos de niri settings"),
    ("Open", "Ouvrir"),
    ("Apply", "Appliquer"),
    ("Add", "Ajouter"),
    ("Save", "Enregistrer"),
    ("Close", "Fermer"),
    ("Cancel", "Annuler"),
    ("Revert", "Rétablir"),
    ("Keep changes", "Conserver les changements"),
    ("Reset to the niri default", "Rétablir la valeur niri"),
    ("About this setting", "À propos de ce réglage"),
    ("Your setup", "Votre configuration"),
    ("At a glance", "En un coup d'œil"),
    ("Recent changes", "Changements récents"),
    ("From another compositor", "Depuis un autre compositeur"),
    ("Problems", "Problèmes"),
    ("Check", "Vérifier"),
    ("Import", "Importer"),
    ("Main menu", "Menu principal"),
    ("Display on", "Écran allumé"),
];

const TR: &[(&str, &str)] = &[
    (
        "This is the system-wide config. Make a personal copy to change it.",
        "Bu, sistem genelindeki yapılandırma. Değiştirmek için kişisel bir kopya oluşturun.",
    ),
    ("Make a personal copy", "Kişisel kopya oluştur"),
    (
        "Turn on {name} to use this.",
        "Bunu kullanmak için {name} özelliğini açın.",
    ),
    ("{n} changes", "{n} değişiklik"),
    ("Undid {n} changes", "{n} değişiklik geri alındı"),
    ("Redid {n} changes", "{n} değişiklik yinelendi"),
    ("Style", "Stil"),
    ("Input", "Giriş"),
    ("System", "Sistem"),
    ("Advanced", "Gelişmiş"),
    ("Pages", "Sayfalar"),
    ("No display positions are available.", "Ekran konumu yok."),
    (
        "Drag a display. Edges snap. The change applies when you release.",
        "Bir ekranı sürükleyin. Kenarlar hizalanır. Değişiklik bırakınca uygulanır.",
    ),
    (
        "Release to apply. niri settings asks you to keep the change.",
        "Uygulamak için bırakın. niri settings değişikliği korumanızı sorar.",
    ),
    ("Search results", "Arama sonuçları"),
    ("Home", "Ana sayfa"),
    ("Appearance", "Görünüm"),
    ("Windows and layout", "Pencereler ve düzen"),
    ("Apps", "Uygulamalar"),
    ("Keyboard", "Klavye"),
    ("Shortcuts", "Kısayollar"),
    ("Mouse and touchpad", "Fare ve dokunmatik yüzey"),
    ("Displays", "Ekranlar"),
    ("Workspaces", "Çalışma alanları"),
    ("Animations", "Animasyonlar"),
    ("Startup", "Başlangıç"),
    ("Check my setup", "Kurulumu denetle"),
    ("All options", "Tüm seçenekler"),
    ("Config file", "Yapılandırma dosyası"),
    ("Search settings", "Ayar ara"),
    ("Undo", "Geri al"),
    ("Redo", "Yinele"),
    ("History", "Geçmiş"),
    ("Preferences", "Tercihler"),
    (
        "Import from Hyprland or Sway",
        "Hyprland veya Sway'den içe aktar",
    ),
    ("Copy as Nix", "Nix olarak kopyala"),
    ("About niri settings", "niri settings hakkında"),
    ("Open", "Aç"),
    ("Apply", "Uygula"),
    ("Add", "Ekle"),
    ("Save", "Kaydet"),
    ("Close", "Kapat"),
    ("Cancel", "Vazgeç"),
    ("Revert", "Geri dön"),
    ("Keep changes", "Değişiklikleri koru"),
    ("Reset to the niri default", "niri varsayılanına dön"),
    ("About this setting", "Bu ayar hakkında"),
    ("Your setup", "Kurulumunuz"),
    ("At a glance", "Bir bakışta"),
    ("Recent changes", "Son değişiklikler"),
    ("From another compositor", "Başka bir kompozitörden"),
    ("Problems", "Sorunlar"),
    ("Check", "Denetle"),
    ("Import", "İçe aktar"),
    ("Main menu", "Ana menü"),
    ("Display on", "Ekran açık"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_fallback() {
        assert_eq!(tr("Home"), "Home");
    }

    #[test]
    fn the_main_pages_are_translated() {
        for (table, home) in [(DE, "Start"), (FR, "Accueil"), (TR, "Ana sayfa")] {
            assert!(
                table
                    .iter()
                    .any(|(source, text)| *source == "Home" && *text == home)
            );
        }
    }

    #[test]
    fn every_language_has_the_main_pages() {
        for table in [DE, FR, TR] {
            for page in [
                "Home",
                "Appearance",
                "Shortcuts",
                "Displays",
                "Config file",
                "Style",
                "Input",
                "System",
                "Advanced",
            ] {
                assert!(
                    table.iter().any(|(source, _)| *source == page),
                    "{page} is missing"
                );
            }
        }
    }
}
