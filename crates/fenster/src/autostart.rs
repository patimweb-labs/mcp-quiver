// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Autostart beim Anmelden — immer mit `--hintergrund`, also Tray statt Fenster.
//!
//! Drei Systeme, drei Ablagen (der Zustand wird **aus dem System gelesen**,
//! nicht nur gemerkt):
//!
//! * macOS: LaunchAgent `~/Library/LaunchAgents/<kennung>.plist`, `RunAtLoad`
//! * Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, Wert `mcp-quiver`
//! * Linux: `~/.config/autostart/<kennung>.desktop`
//!
//! `QUIVER_AUTOSTART_DIR` verlegt das Zielverzeichnis (Tests, andere Sitzungen)
//! — auf macOS und Linux. Die Inhalte entstehen in reinen Funktionen
//! ([`plist`], [`desktop`]), damit sie sich ohne System prüfen lassen.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Kennung: Name der plist- und der .desktop-Datei.
pub const KENNUNG: &str = "io.github.patimweb-labs.quiver";

/// Das Argument, mit dem quiver ohne Fenster startet.
pub const ARGUMENT: &str = "--hintergrund";

/// Umgebungsvariable, die das Zielverzeichnis verlegt (Tests).
pub const UMGEBUNG: &str = "QUIVER_AUTOSTART_DIR";

/// Dateiname der Autostart-Datei auf dieser Plattform.
pub fn dateiname() -> &'static str {
    if cfg!(target_os = "macos") {
        "io.github.patimweb-labs.quiver.plist"
    } else {
        "io.github.patimweb-labs.quiver.desktop"
    }
}

/// Zielverzeichnis der Autostart-Datei. `None` auf Windows (dort steht der
/// Eintrag in der Registry) und wenn es kein Zuhause gibt.
pub fn verzeichnis() -> Option<PathBuf> {
    verzeichnis_aus(cfg!(target_os = "macos"), |name| std::env::var_os(name))
}

/// Wie [`verzeichnis`], aber mit nachschlagbarer Umgebung (Test).
pub fn verzeichnis_aus(macos: bool, lesen: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if let Some(pfad) = lesen(UMGEBUNG).filter(|pfad| !pfad.is_empty()) {
        return Some(PathBuf::from(pfad));
    }
    let vorhanden = |name: &str| lesen(name).filter(|wert| !wert.is_empty());
    if macos {
        return Some(PathBuf::from(vorhanden("HOME")?).join("Library/LaunchAgents"));
    }
    if let Some(konfig) = vorhanden("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(konfig).join("autostart"));
    }
    Some(PathBuf::from(vorhanden("HOME")?).join(".config/autostart"))
}

/// Pfad der Autostart-Datei (macOS, Linux).
pub fn datei() -> Option<PathBuf> {
    Some(verzeichnis()?.join(dateiname()))
}

/// Das laufende Programm — der Pfad, der im Autostart-Eintrag steht.
pub fn programm() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("quiver"))
}

/// Ist der Autostart eingetragen? Gefragt wird das System, nicht das Fenster.
pub fn aktiv() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::aktiv()
    }
    #[cfg(not(target_os = "windows"))]
    {
        datei().is_some_and(|pfad| pfad.is_file())
    }
}

/// Autostart ein- oder ausschalten.
///
/// Auf Windows steht der Eintrag in der Registry, sonst in einer Datei;
/// deshalb ein Block je Plattform statt eines `return` dazwischen.
pub fn setzen(an: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        windows::setzen(an)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let pfad = datei().ok_or_else(|| anyhow::anyhow!("kein Zuhause für den Autostart"))?;
        setzen_in(&pfad, an, cfg!(target_os = "macos"), &programm())
    }
}

/// Schreibt oder entfernt die Autostart-Datei an `pfad`. `plist` wählt das
/// Format: `true` für macOS, `false` für Linux.
pub fn setzen_in(pfad: &Path, an: bool, plist: bool, programm: &Path) -> anyhow::Result<()> {
    if !an {
        if pfad.is_file() {
            std::fs::remove_file(pfad)?;
        }
        return Ok(());
    }
    if let Some(ordner) = pfad.parent() {
        std::fs::create_dir_all(ordner)?;
    }
    let inhalt = if plist {
        plist_inhalt(programm)
    } else {
        desktop_inhalt(programm)
    };
    std::fs::write(pfad, inhalt)?;
    Ok(())
}

/// Inhalt der launchd-Datei (macOS). `RunAtLoad` startet sie beim Anmelden.
pub fn plist_inhalt(programm: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{KENNUNG}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{}</string>\n\
         \t\t<string>{ARGUMENT}</string>\n\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>ProcessType</key>\n\
         \t<string>Interactive</string>\n\
         </dict>\n\
         </plist>\n",
        xml_frei(&programm.display().to_string())
    )
}

/// Inhalt der .desktop-Datei (Linux). `NoDisplay` hält sie aus dem
/// Anwendungsmenü heraus — sie startet nur.
pub fn desktop_inhalt(programm: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=mcp-quiver\n\
         Comment=mcp-quiver im Hintergrund starten (Tray-Symbol)\n\
         Exec={} {ARGUMENT}\n\
         Terminal=false\n\
         NoDisplay=true\n\
         Hidden=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_pfad(programm)
    )
}

/// Ein Pfad für `Exec=`: mit Leerzeichen in Anführungszeichen, sonst nackt.
pub fn exec_pfad(programm: &Path) -> String {
    let pfad = programm.display().to_string();
    if pfad.contains([' ', '\t', '"']) {
        format!("\"{}\"", pfad.replace('"', "\\\""))
    } else {
        pfad
    }
}

/// `&`, `<` und `>` sind in XML nicht erlaubt — in Pfaden selten, aber möglich.
fn xml_frei(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Windows: ein Wert unter `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
#[cfg(target_os = "windows")]
mod windows {
    use std::path::Path;

    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    use super::{programm, ARGUMENT};

    const SCHLUESSEL: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const WERT: &str = "mcp-quiver";

    fn key() -> anyhow::Result<RegKey> {
        Ok(RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
            SCHLUESSEL,
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )?)
    }

    pub fn aktiv() -> bool {
        key().is_ok_and(|key| key.get_value::<String, _>(WERT).is_ok())
    }

    pub fn setzen(an: bool) -> anyhow::Result<()> {
        let key = key()?;
        if an {
            key.set_value(WERT, &befehl(&programm()))?;
        } else if key.get_value::<String, _>(WERT).is_ok() {
            key.delete_value(WERT)?;
        }
        Ok(())
    }

    /// Der Wert, der beim Anmelden ausgeführt wird.
    pub fn befehl(programm: &Path) -> String {
        format!("\"{}\" {ARGUMENT}", programm.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner(tag: &str) -> PathBuf {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        static ZAEHLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pfad = std::env::temp_dir().join(format!(
            "quiver-autostart-{}-{tag}-{stempel}-{nummer}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    #[test]
    fn plist_traegt_programm_und_hintergrund() {
        let text = plist_inhalt(Path::new("/Applications/quiver"));
        assert!(text.starts_with("<?xml version=\"1.0\""), "{text}");
        assert!(text.contains(&format!("<string>{KENNUNG}</string>")));
        assert!(
            text.contains("<string>/Applications/quiver</string>"),
            "Programm fehlt"
        );
        assert!(
            text.contains(&format!("<string>{ARGUMENT}</string>")),
            "ohne --hintergrund startet es mit Fenster"
        );
        assert!(text.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(text.ends_with("</plist>\n"));
        // Ausgewogenes XML: ebenso viele öffnende wie schließende dict/array.
        assert_eq!(
            text.matches("<dict>").count(),
            text.matches("</dict>").count()
        );
        assert_eq!(
            text.matches("<array>").count(),
            text.matches("</array>").count()
        );
    }

    #[test]
    fn plist_entschaerft_sonderzeichen() {
        let text = plist_inhalt(Path::new("/tmp/a & b/<quiver>"));
        assert!(text.contains("<string>/tmp/a &amp; b/&lt;quiver&gt;</string>"));
    }

    #[test]
    fn desktop_traegt_programm_und_hintergrund() {
        let text = desktop_inhalt(Path::new("/usr/bin/quiver"));
        assert!(text.starts_with("[Desktop Entry]\n"), "{text}");
        assert!(
            text.contains(&format!("Exec=/usr/bin/quiver {ARGUMENT}\n")),
            "Exec fehlt"
        );
        assert!(text.contains("X-GNOME-Autostart-enabled=true\n"));
        assert!(
            text.contains("NoDisplay=true\n"),
            "kein zweiter Menüeintrag"
        );
    }

    #[test]
    fn exec_pfad_setzt_leerzeichen_in_anfuehrungszeichen() {
        assert_eq!(exec_pfad(Path::new("/usr/bin/quiver")), "/usr/bin/quiver");
        assert_eq!(
            exec_pfad(Path::new("/Applications/Mein Quiver/quiver")),
            "\"/Applications/Mein Quiver/quiver\""
        );
    }

    #[test]
    fn verzeichnis_folgt_der_umgebung() {
        let umgebung = |werte: Vec<(&'static str, &'static str)>| {
            move |name: &str| {
                werte
                    .iter()
                    .find(|(schluessel, _)| *schluessel == name)
                    .map(|(_, wert)| OsString::from(*wert))
            }
        };
        // Die eigene Variable gilt zuerst — auf allen Plattformen.
        assert_eq!(
            verzeichnis_aus(true, umgebung(vec![(UMGEBUNG, "/tmp/test")])),
            Some(PathBuf::from("/tmp/test"))
        );
        assert_eq!(
            verzeichnis_aus(
                true,
                umgebung(vec![("HOME", "/Users/ich"), (UMGEBUNG, "/tmp/test")])
            ),
            Some(PathBuf::from("/tmp/test"))
        );
        // macOS: LaunchAgents im Zuhause.
        assert_eq!(
            verzeichnis_aus(true, umgebung(vec![("HOME", "/Users/ich")])),
            Some(PathBuf::from("/Users/ich/Library/LaunchAgents"))
        );
        // Linux: XDG vor dem Zuhause.
        assert_eq!(
            verzeichnis_aus(
                false,
                umgebung(vec![
                    ("HOME", "/home/ich"),
                    ("XDG_CONFIG_HOME", "/home/ich/.config")
                ])
            ),
            Some(PathBuf::from("/home/ich/.config/autostart"))
        );
        assert_eq!(
            verzeichnis_aus(false, umgebung(vec![("HOME", "/home/ich")])),
            Some(PathBuf::from("/home/ich/.config/autostart"))
        );
        // Ohne Zuhause und ohne eigene Variable: kein Ziel.
        assert_eq!(verzeichnis_aus(false, umgebung(vec![])), None);
    }

    #[test]
    fn dateiname_traegt_die_kennung() {
        let name = dateiname();
        assert!(name.starts_with(KENNUNG), "{name}");
        assert!(
            name.ends_with(if cfg!(target_os = "macos") {
                ".plist"
            } else {
                ".desktop"
            }),
            "{name}"
        );
    }

    #[test]
    fn setzen_in_schreibt_und_entfernt() {
        let ordner = ordner("schreiben");
        let ziel = ordner.join("autostart").join(dateiname());
        let programm = Path::new("/usr/bin/quiver");
        let plist = true;

        assert!(!ziel.is_file());
        setzen_in(&ziel, true, plist, programm).expect("schreibbar");
        assert!(ziel.is_file());
        let text = std::fs::read_to_string(&ziel).expect("lesbar");
        assert_eq!(
            text,
            plist_inhalt(programm),
            "derselbe Inhalt wie die reine Funktion"
        );
        // Erneut einschalten ist folgenlos.
        setzen_in(&ziel, true, plist, programm).expect("schreibbar");

        setzen_in(&ziel, false, plist, programm).expect("entfernbar");
        assert!(!ziel.is_file());
        // Ausschalten ohne Datei ist ebenfalls folgenlos.
        setzen_in(&ziel, false, plist, programm).expect("entfernbar");

        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn setzen_in_schreibt_desktop_auf_wunsch() {
        let ordner = ordner("desktop");
        let ziel = ordner.join("quiver.desktop");
        setzen_in(&ziel, true, false, Path::new("/usr/bin/quiver")).expect("schreibbar");
        let text = std::fs::read_to_string(&ziel).expect("lesbar");
        assert_eq!(text, desktop_inhalt(Path::new("/usr/bin/quiver")));
        std::fs::remove_dir_all(&ordner).ok();
    }
}
