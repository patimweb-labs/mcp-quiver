// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Tausch selbst (Aufgabe 39).
//!
//! Reihenfolge, immer dieselbe:
//!
//! 1. **Entpacken** in einen Arbeitsordner im Profil (nie über das laufende
//!    Programm).
//! 2. **Selbsttest** der neuen Fassung (`--selbsttest`, Exit 0, ohne Fenster,
//!    ohne Profil). Scheitert er, bleibt alles, wie es ist.
//! 3. **Tauschen**: die alte Fassung wird umbenannt (`<name>.alt`), die neue
//!    daneben gelegt. Die Sicherung bleibt liegen, bis die neue Fassung ihren
//!    Start bewiesen hat (Marke im Profil, siehe [`marke_schreiben`]).
//! 4. **Neustart vormerken**: die neue Fassung wird gestartet, sobald dieser
//!    Prozess das Profil freigegeben hat (`neustart.json`).
//!
//! Das Profil selbst wird **nie** angefasst: es liegt außerhalb des
//! Programms, und der Tausch kennt nur den Programmpfad. Ein Test vergleicht
//! Profil, Tresor und Geräte-Token vor und nach dem Tausch.

use std::path::{Path, PathBuf};

use quiver_domain::updates::PaketArt;

use super::art::Installationsart;
use super::entpacken;

/// So heißt die Sicherung neben dem Programm: `quiver.alt`.
pub const SICHERUNG_ENDUNG: &str = "alt";
/// Die Marke im Profil, solange die neue Fassung ihren Start nicht bewiesen hat.
pub const MARKE: &str = "update-offen.json";
/// Die Bitte um einen Neustart, die der nächste Start des Fensters aufnimmt.
pub const NEUSTART: &str = "update-neustart.json";

/// Was der Tausch ergeben hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ergebnis {
    /// Getauscht. `sicherung` ist die alte Fassung (bleibt bis zum Beweis).
    Getauscht {
        programm: PathBuf,
        sicherung: PathBuf,
    },
    /// Das MSI muss laufen: der Client beendet sich, das MSI startet ihn
    /// wieder (Restart Manager, siehe `verteilung/windows/quiver.wxs`).
    Installer { programm: PathBuf, paket: PathBuf },
}

/// Welche Paketart zu welcher Installation gehört.
pub fn paketart_fuer(art: Installationsart) -> Option<PaketArt> {
    match art {
        Installationsart::Zip => Some(PaketArt::Zip),
        Installationsart::Msi => Some(PaketArt::Msi),
        Installationsart::AppBundle => Some(PaketArt::Dmg),
        Installationsart::AppImage => Some(PaketArt::AppImage),
        Installationsart::TarGz => Some(PaketArt::TarGz),
        Installationsart::Deb => Some(PaketArt::Deb),
        Installationsart::Rpm => Some(PaketArt::Rpm),
        Installationsart::Unbekannt => None,
    }
}

/// Eine neue Fassung entpacken und prüfen, **ohne** zu tauschen.
///
/// `Ok(None)` heißt: für diese Art gibt es nichts zu entpacken (AppImage, MSI).
/// `Ok(Some(pfad))` ist das neue Programm, das den Selbsttest bestanden hat;
/// `Err` heißt: es bleibt alles, wie es ist.
pub fn vorbereiten(
    art: Installationsart,
    paket: &Path,
    arbeitsordner: &Path,
    selbsttest: impl Fn(&Path) -> bool,
) -> anyhow::Result<Option<PathBuf>> {
    match art {
        Installationsart::Zip => {
            let bytes = std::fs::read(paket)?;
            let baum = arbeitsordner.join("neu");
            entpacken::zip_entpacken(&bytes, &baum)?;
            let neu = entpacken::finde_datei(&baum, programmname())
                .ok_or_else(|| anyhow::anyhow!("im Paket liegt kein Programm"))?;
            // Aus einem Archiv kommt kein Ausführungsrecht: das setzt der
            // Tausch, sonst könnte der Selbsttest die Fassung nicht starten.
            ausfuehrbar(&neu)?;
            selbsttest_oder_abbruch(&neu, &selbsttest)?;
            Ok(Some(neu))
        }
        Installationsart::TarGz => {
            let bytes = std::fs::read(paket)?;
            let baum = arbeitsordner.join("neu");
            entpacken::tar_gz_entpacken(&bytes, &baum)?;
            let neu = entpacken::finde_datei(&baum, programmname())
                .ok_or_else(|| anyhow::anyhow!("im Paket liegt kein Programm"))?;
            ausfuehrbar(&neu)?;
            selbsttest_oder_abbruch(&neu, &selbsttest)?;
            Ok(Some(neu))
        }
        Installationsart::AppImage => {
            let baum = arbeitsordner.join("neu");
            std::fs::create_dir_all(&baum)?;
            let neu = baum.join(programmname());
            std::fs::copy(paket, &neu)?;
            ausfuehrbar(&neu)?;
            selbsttest_oder_abbruch(&neu, &selbsttest)?;
            Ok(Some(neu))
        }
        Installationsart::AppBundle => {
            let neu = bundel_aus_dmg(paket, arbeitsordner)?;
            let programm = neu.join("Contents").join("MacOS").join(programmname());
            selbsttest_oder_abbruch(&programm, &selbsttest)?;
            Ok(Some(neu))
        }
        // Das MSI ist ein Installationsprogramm: es enthält das Programm, aber
        // es zu entpacken hieße, `msiexec` nachzubauen. Getestet wird es von
        // Windows selbst (`/qn`, `MajorUpgrade`, Restart Manager); der
        // Selbsttest der neuen Fassung folgt beim ersten Start.
        Installationsart::Msi => Ok(None),
        Installationsart::Deb | Installationsart::Rpm | Installationsart::Unbekannt => Ok(None),
    }
}

/// Den Tausch ausführen: vorbereiten, prüfen, tauschen.
pub fn tauschen(
    art: Installationsart,
    programm: &Path,
    paket: &Path,
    arbeitsordner: &Path,
    selbsttest: impl Fn(&Path) -> bool,
) -> anyhow::Result<Ergebnis> {
    if !art.tauschbar() {
        anyhow::bail!(
            "diese Installation tauscht quiver nicht selbst ({})",
            art.als_text()
        );
    }
    std::fs::create_dir_all(arbeitsordner)?;
    let neu = vorbereiten(art, paket, arbeitsordner, selbsttest)?;
    anwenden(art, programm, paket, neu)
}

/// Den zweiten Schritt allein: was [`vorbereiten`] geprüft hat, an seinen Platz
/// legen. Die alte Fassung bleibt als Sicherung liegen.
pub fn anwenden(
    art: Installationsart,
    programm: &Path,
    paket: &Path,
    neu: Option<PathBuf>,
) -> anyhow::Result<Ergebnis> {
    match art {
        Installationsart::Msi => Ok(Ergebnis::Installer {
            programm: programm.to_path_buf(),
            paket: paket.to_path_buf(),
        }),
        Installationsart::AppBundle => {
            let neu = neu.ok_or_else(|| anyhow::anyhow!("das Bündel fehlt"))?;
            let sicherung = ordner_tauschen(&neu, programm)?;
            Ok(Ergebnis::Getauscht {
                programm: programm.to_path_buf(),
                sicherung,
            })
        }
        _ => {
            let neu = neu.ok_or_else(|| anyhow::anyhow!("die neue Fassung fehlt"))?;
            let sicherung = datei_tauschen(&neu, programm)?;
            Ok(Ergebnis::Getauscht {
                programm: programm.to_path_buf(),
                sicherung,
            })
        }
    }
}

/// Der Programmname auf diesem System.
pub fn programmname() -> &'static str {
    if cfg!(windows) {
        "quiver.exe"
    } else {
        "quiver"
    }
}

fn selbsttest_oder_abbruch(neu: &Path, selbsttest: &impl Fn(&Path) -> bool) -> anyhow::Result<()> {
    if selbsttest(neu) {
        Ok(())
    } else {
        anyhow::bail!("die neue Fassung hat den Selbsttest nicht bestanden; nichts getauscht")
    }
}

/// Eine Datei ausführbar machen (Unix: 0755; Windows: nichts nötig).
fn ausfuehrbar(pfad: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut rechte = std::fs::metadata(pfad)?.permissions();
        rechte.set_mode(0o755);
        std::fs::set_permissions(pfad, rechte)?;
    }
    #[cfg(not(unix))]
    {
        let _ = pfad;
    }
    Ok(())
}

/// Die Sicherung neben einer Datei: `quiver.alt`, `quiver.exe.alt`.
pub fn sicherung_pfad(programm: &Path) -> PathBuf {
    let name = programm
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("quiver"));
    programm.with_file_name(format!("{name}.{SICHERUNG_ENDUNG}"))
}

/// Eine Datei tauschen: alte umbenennen, neue daneben legen.
///
/// Scheitert das Ablegen, kommt die alte Fassung zurück – der Ordner ist dann
/// wieder wie vorher.
fn datei_tauschen(neu: &Path, ziel: &Path) -> anyhow::Result<PathBuf> {
    let sicherung = sicherung_pfad(ziel);
    // Eine Sicherung von früher: sie ist nicht mehr nötig (die neue Fassung
    // lief), wird aber nur entfernt, wenn sie darf.
    if sicherung.exists() {
        let _ = std::fs::remove_file(&sicherung);
    }
    std::fs::rename(ziel, &sicherung).map_err(|fehler| {
        anyhow::anyhow!("die alte Fassung lässt sich nicht umbenennen: {fehler}")
    })?;
    let kopiert = std::fs::copy(neu, ziel)
        .map_err(|fehler| anyhow::anyhow!("die neue Fassung lässt sich nicht ablegen: {fehler}"));
    match kopiert {
        Ok(_) => {
            ausfuehrbar(ziel)?;
            Ok(sicherung)
        }
        Err(fehler) => {
            let _ = std::fs::rename(&sicherung, ziel);
            Err(fehler)
        }
    }
}

/// Ein Bündel tauschen (macOS): alten Ordner umbenennen, neuen hinstellen.
fn ordner_tauschen(neu: &Path, ziel: &Path) -> anyhow::Result<PathBuf> {
    let sicherung = ziel.with_file_name(format!(
        "{}.{SICHERUNG_ENDUNG}",
        ziel.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| String::from("quiver.app"))
    ));
    if sicherung.exists() {
        let _ = std::fs::remove_dir_all(&sicherung);
    }
    std::fs::rename(ziel, &sicherung).map_err(|fehler| {
        anyhow::anyhow!("das alte Bündel lässt sich nicht umbenennen: {fehler}")
    })?;
    match std::fs::rename(neu, ziel) {
        Ok(()) => Ok(sicherung),
        Err(fehler) => {
            let _ = std::fs::rename(&sicherung, ziel);
            Err(anyhow::anyhow!(
                "das neue Bündel lässt sich nicht hinstellen: {fehler}"
            ))
        }
    }
}

// -------------------------------------------------------------- macOS/DMG

/// Das Bündel aus einem `.dmg` holen – mit Bordmitteln (`hdiutil`, `ditto`).
#[cfg(target_os = "macos")]
fn bundel_aus_dmg(dmg: &Path, arbeitsordner: &Path) -> anyhow::Result<PathBuf> {
    use std::process::Command;
    let baum = arbeitsordner.join("neu");
    std::fs::create_dir_all(&baum)?;
    let berg = arbeitsordner.join("dmg");
    std::fs::create_dir_all(&berg)?;
    let status = Command::new("hdiutil")
        .arg("attach")
        .arg("-nobrowse")
        .arg("-readonly")
        .arg("-mountpoint")
        .arg(&berg)
        .arg(dmg)
        .status()
        .map_err(|fehler| anyhow::anyhow!("hdiutil nicht verfügbar: {fehler}"))?;
    if !status.success() {
        anyhow::bail!("das .dmg lässt sich nicht einhängen");
    }
    let ergebnis = (|| {
        let bundel = std::fs::read_dir(&berg)?
            .flatten()
            .map(|eintrag| eintrag.path())
            .find(|pfad| {
                pfad.extension()
                    .is_some_and(|endung| endung.eq_ignore_ascii_case("app"))
            })
            .ok_or_else(|| anyhow::anyhow!("im .dmg liegt kein .app"))?;
        let neu = baum.join("quiver.app");
        let status = Command::new("ditto")
            .arg(&bundel)
            .arg(&neu)
            .status()
            .map_err(|fehler| anyhow::anyhow!("ditto nicht verfügbar: {fehler}"))?;
        if !status.success() {
            anyhow::bail!("das Bündel lässt sich nicht kopieren");
        }
        Ok(neu)
    })();
    let _ = Command::new("hdiutil").arg("detach").arg(&berg).status();
    ergebnis
}

/// Auf allen anderen Systemen ist ein `.dmg` kein Paket für dieses Gerät.
#[cfg(not(target_os = "macos"))]
fn bundel_aus_dmg(_dmg: &Path, _arbeitsordner: &Path) -> anyhow::Result<PathBuf> {
    anyhow::bail!("ein .dmg gibt es nur für macOS")
}

/// Ein MSI still starten: der Client beendet sich danach, das MSI ersetzt die
/// Fassung und startet ihn wieder (Restart Manager).
pub fn installer_starten(paket: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let kind = std::process::Command::new("msiexec")
            .arg("/i")
            .arg(paket)
            // Still, ohne Neustart des Rechners, ohne Nachfrage.
            .arg("/qn")
            .arg("/norestart")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|fehler| anyhow::anyhow!("msiexec lässt sich nicht starten: {fehler}"))?;
        tracing::info!(paket = %paket.display(), "MSI läuft (PID {})", kind.id());
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = paket;
        anyhow::bail!("ein MSI gibt es nur für Windows")
    }
}

// --------------------------------------------------- Marke und Neustart

/// Die Marke im Profil: „die neue Fassung ist getauscht und muss ihren Start
/// noch beweisen“.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Marke {
    /// Was ersetzt wurde.
    pub ziel: PathBuf,
    /// Die alte Fassung daneben.
    pub sicherung: PathBuf,
    /// Wie oft die neue Fassung schon starten sollte.
    #[serde(default)]
    pub versuche: u32,
}

fn marke_pfad(profil: &Path) -> PathBuf {
    profil.join(MARKE)
}

/// Nach dem Tausch die Marke schreiben.
pub fn marke_schreiben(profil: &Path, ziel: &Path, sicherung: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(profil)?;
    let marke = Marke {
        ziel: ziel.to_path_buf(),
        sicherung: sicherung.to_path_buf(),
        versuche: 0,
    };
    let text = serde_json::to_string(&marke)?;
    let zwischen = marke_pfad(profil).with_extension("json.neu");
    std::fs::write(&zwischen, text)?;
    std::fs::rename(&zwischen, marke_pfad(profil))?;
    Ok(())
}

/// Beim Start: ist die neue Fassung schon einmal **nicht** gekommen?
///
/// Der erste Start nach dem Tausch zählt eins und läuft normal weiter. Bleibt
/// die Marke danach stehen (die neue Fassung kam über diesen Punkt nicht
/// hinaus), zählt der nächste Start zwei – dann wird die alte Fassung wieder
/// hingestellt, und der Aufrufer startet sie. `Some(pfad)` ist das Programm,
/// das dafür gestartet werden soll.
pub fn rueckfall(profil: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(marke_pfad(profil)).ok()?;
    let mut marke: Marke = serde_json::from_str(&text).ok()?;
    if marke.versuche == 0 {
        marke.versuche = 1;
        let _ = std::fs::write(
            marke_pfad(profil),
            serde_json::to_string(&marke).ok().unwrap_or_default(),
        );
        return None;
    }
    // Zweiter Versuch: die Sicherung zurückholen.
    if !marke.sicherung.exists() {
        let _ = std::fs::remove_file(marke_pfad(profil));
        return None;
    }
    // Die kaputte Fassung wegräumen, dann die alte wieder hinstellen.
    if marke.ziel.is_dir() {
        let _ = std::fs::remove_dir_all(&marke.ziel);
        if std::fs::rename(&marke.sicherung, &marke.ziel).is_ok() {
            let _ = std::fs::remove_file(marke_pfad(profil));
            tracing::warn!(
                "die neue Fassung ist nicht gestartet; zurück auf {}",
                marke.ziel.display()
            );
            return Some(marke.ziel);
        }
        return None;
    }
    let _ = std::fs::remove_file(&marke.ziel);
    if std::fs::copy(&marke.sicherung, &marke.ziel).is_err() {
        return None;
    }
    let _ = ausfuehrbar(&marke.ziel);
    let _ = std::fs::remove_file(marke_pfad(profil));
    tracing::warn!(
        "die neue Fassung ist nicht gestartet; zurück auf {}",
        marke.ziel.display()
    );
    Some(marke.ziel)
}

/// Die neue Fassung hat ihren Start bewiesen: Marke und Sicherung weg.
pub fn bestaetigen(profil: &Path) {
    let Ok(text) = std::fs::read_to_string(marke_pfad(profil)) else {
        return;
    };
    if let Ok(marke) = serde_json::from_str::<Marke>(&text) {
        if marke.sicherung.is_file() {
            let _ = std::fs::remove_file(&marke.sicherung);
        } else if marke.sicherung.is_dir() {
            let _ = std::fs::remove_dir_all(&marke.sicherung);
        }
    }
    let _ = std::fs::remove_file(marke_pfad(profil));
}

/// Den Neustart vormerken: das übernimmt der nächste Start des Fensters,
/// nachdem dieser Prozess das Profil freigegeben hat.
pub fn neustart_vormerken(profil: &Path, programm: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(profil)?;
    std::fs::write(
        profil.join(NEUSTART),
        serde_json::to_string(&programm.to_path_buf())?,
    )?;
    Ok(())
}

/// Einen vorgemerkten Neustart übernehmen (einmal).
pub fn neustart_uebernehmen(profil: &Path) -> Option<PathBuf> {
    let pfad = profil.join(NEUSTART);
    let text = std::fs::read_to_string(&pfad).ok()?;
    let _ = std::fs::remove_file(&pfad);
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests;
