// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Symbol des Tray-Eintrags. Ein Logo gibt es noch nicht (AGENTS.md §8);
//! bis dahin ist es das Lucide-Zeichen `package` aus `oberflaeche/icons` —
//! dasselbe Bild wie die Seite „Tools". Gerastert wird mit `resvg` in 32 und
//! 64 px. Später wird nur die Bilddatei getauscht, nicht dieser Code.

/// Das Zeichen als SVG, wie es die Oberfläche benutzt.
pub const SVG: &str = include_str!("../../../oberflaeche/icons/package.svg");

/// Die beiden Größen, die die Plattformen wollen: macOS nimmt das kleine Bild,
/// Windows und Linux skalieren selbst.
pub const KANTEN: [u32; 2] = [32, 64];

/// Das Zeichen als rohe RGBA-Werte (gerade, nicht vormultipliziert), wie
/// `tray-icon` und `ksni` es erwarten.
pub fn raster(kante: u32) -> anyhow::Result<Vec<u8>> {
    let baum = resvg::usvg::Tree::from_str(SVG, &resvg::usvg::Options::default())
        .map_err(|fehler| anyhow::anyhow!("Tray-Symbol ist kein SVG: {fehler}"))?;
    let mut bild = resvg::tiny_skia::Pixmap::new(kante, kante)
        .ok_or_else(|| anyhow::anyhow!("Tray-Symbol mit {kante} px nicht anlegbar"))?;
    // Das Zeichen ist quadratisch (24 × 24); die Kante bestimmt den Faktor.
    let faktor = kante as f32 / baum.size().width().max(1.0);
    resvg::render(
        &baum,
        resvg::tiny_skia::Transform::from_scale(faktor, faktor),
        &mut bild.as_mut(),
    );
    Ok(entmultipliziert(bild.data()))
}

/// `tiny-skia` rechnet mit vormultiplizierten Werten (Farbe × Deckkraft);
/// `tray-icon` und `ksni` wollen die Farbe unabhängig von der Deckkraft —
/// sonst bekämen die weichen Kanten einen dunklen Saum.
fn entmultipliziert(daten: &[u8]) -> Vec<u8> {
    daten
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let deckkraft = pixel[3] as u32;
            match deckkraft {
                0 => [0, 0, 0, 0],
                255 => *pixel,
                _ => {
                    let zurueck =
                        |wert: u8| ((wert as u32 * 255 + deckkraft / 2) / deckkraft) as u8;
                    [
                        zurueck(pixel[0]),
                        zurueck(pixel[1]),
                        zurueck(pixel[2]),
                        pixel[3],
                    ]
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jede_kante_hat_ihre_pixel() {
        for kante in KANTEN {
            let bild = raster(kante).expect("Symbol rasterbar");
            assert_eq!(bild.len(), (kante * kante * 4) as usize);
        }
    }

    #[test]
    fn das_zeichen_ist_sichtbar_und_durchsichtig_ringsherum() {
        let kante = 32;
        let bild = raster(kante).expect("Symbol rasterbar");
        let deckkraft = |x: u32, y: u32| bild[((y * kante + x) * 4 + 3) as usize];
        // Die Ecke bleibt leer, in der Mitte steht etwas.
        assert_eq!(deckkraft(0, 0), 0, "Ecke ist durchsichtig");
        assert_eq!(deckkraft(kante - 1, kante - 1), 0, "Ecke ist durchsichtig");
        let summe: u32 = (0..kante * kante)
            .map(|i| bild[(i * 4 + 3) as usize] as u32)
            .sum();
        assert!(summe > 0, "das Zeichen ist unsichtbar");
    }

    #[test]
    fn entmultiplizieren_hebt_die_farbe_an() {
        // Halbdurchsichtiges Weiß: vormultipliziert 128, gerade 255.
        assert_eq!(
            entmultipliziert(&[128, 128, 128, 128]),
            vec![255, 255, 255, 128]
        );
        // Voll und leer bleiben, wie sie sind.
        assert_eq!(entmultipliziert(&[0, 0, 0, 0]), vec![0, 0, 0, 0]);
        assert_eq!(entmultipliziert(&[9, 8, 7, 255]), vec![9, 8, 7, 255]);
    }
}
