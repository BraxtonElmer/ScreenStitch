//! Just enough EDID parsing to learn a panel's physical size and identity.

pub struct Edid {
    /// Three-letter manufacturer code, e.g. "GSM".
    pub manufacturer: String,
    pub product: u16,
    pub serial: Option<String>,
    pub name: Option<String>,
    /// Physical image size in mm, as the panel reports it (not rotated).
    pub size_mm: Option<(f64, f64)>,
}

pub fn parse(b: &[u8]) -> Option<Edid> {
    if b.len() < 128 || b[..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return None;
    }
    let m = u16::from_be_bytes([b[8], b[9]]);
    let letter = |v: u16| char::from(b'A' - 1 + (v & 0x1F) as u8);
    let manufacturer: String = [letter(m >> 10), letter(m >> 5), letter(m)].iter().collect();
    let product = u16::from_le_bytes([b[10], b[11]]);
    let serial_num = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);

    let mut name = None;
    let mut serial_text = None;
    for d in (54..=108).step_by(18) {
        let desc = &b[d..d + 18];
        if desc[0] == 0 && desc[1] == 0 {
            let text = || {
                let s: String = desc[5..18].iter().take_while(|&&c| c != 0x0A).map(|&c| c as char).collect();
                Some(s.trim().to_string()).filter(|s| !s.is_empty())
            };
            match desc[3] {
                0xFC => name = text(),
                0xFF => serial_text = text(),
                _ => {}
            }
        }
    }

    // Detailed timing descriptor sizes are in mm; the basic block only in cm.
    let dtd = &b[54..72];
    let dtd_size = (dtd[0] != 0 || dtd[1] != 0).then(|| {
        let w = u16::from(dtd[12]) | (u16::from(dtd[14] & 0xF0) << 4);
        let h = u16::from(dtd[13]) | (u16::from(dtd[14] & 0x0F) << 8);
        (f64::from(w), f64::from(h))
    });
    let cm_size = (b[21] != 0 && b[22] != 0).then(|| (f64::from(b[21]) * 10.0, f64::from(b[22]) * 10.0));
    let size_mm = [dtd_size, cm_size].into_iter().flatten().find(|&(w, h)| plausible(w, h));

    Some(Edid {
        manufacturer,
        product,
        serial: serial_text.or((serial_num != 0).then(|| format!("{serial_num:08X}"))),
        name,
        size_mm,
    })
}

/// TVs and projectors often report 0, 1 cm, or an aspect ratio code instead of a size.
fn plausible(w: f64, h: f64) -> bool {
    (80.0..=3000.0).contains(&w) && (50.0..=3000.0).contains(&h) && (0.2..=5.0).contains(&(w / h))
}

/// Many monitors only report whole centimetres (e.g. 600 x 340 mm for a 27"
/// panel that is really 597.7 x 336.2 mm). Half a centimetre off is enough to
/// put crossings a millimetre or two off, so such sizes are rebuilt from the
/// panel's diagonal (snapped to the nearest standard size) and the exact shape
/// of its pixel grid.
pub fn refine_rounded_size(size: (f64, f64), pixels: (f64, f64)) -> (f64, f64) {
    let (w, h) = size;
    let rounded = w % 10.0 == 0.0 && h % 10.0 == 0.0;
    if !rounded || pixels.0 <= 0.0 || pixels.1 <= 0.0 {
        return size;
    }
    const STANDARD_INCHES: [f64; 34] = [
        13.3, 14.0, 15.6, 17.0, 17.3, 18.5, 19.0, 19.5, 20.0, 21.5, 22.0, 23.0, 23.6, 23.8, 24.0, 24.5, 25.0, 27.0,
        28.0, 29.0, 30.0, 31.5, 32.0, 34.0, 35.0, 38.0, 40.0, 42.0, 43.0, 45.0, 48.0, 49.0, 55.0, 65.0,
    ];
    let inches = w.hypot(h) / 25.4;
    let nearest = STANDARD_INCHES.iter().copied().min_by(|a, b| (a - inches).abs().total_cmp(&(b - inches).abs()));
    // Whole-centimetre rounding moves the diagonal by at most ~0.3".
    let inches = nearest.filter(|n| (n - inches).abs() <= 0.35).unwrap_or(inches);
    let aspect = pixels.0 / pixels.1;
    let diag = inches * 25.4;
    let h = diag / aspect.hypot(1.0);
    (h * aspect, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        // "GSM" = G(7) S(19) M(13)
        let m: u16 = (7 << 10) | (19 << 5) | 13;
        b[8..10].copy_from_slice(&m.to_be_bytes());
        b[10..12].copy_from_slice(&0x5B09u16.to_le_bytes());
        b[21] = 60;
        b[22] = 34;
        // DTD with 597 x 336 mm
        b[54] = 0x01;
        b[66] = (597 & 0xFF) as u8;
        b[67] = (336 & 0xFF) as u8;
        b[68] = (((597 >> 8) as u8) << 4) | ((336 >> 8) as u8);
        // Name descriptor
        b[72..77].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        b[77..88].copy_from_slice(b"LG HDR 4K\n ");
        b
    }

    #[test]
    fn reads_identity_and_size() {
        let e = parse(&sample()).unwrap();
        assert_eq!(e.manufacturer, "GSM");
        assert_eq!(e.product, 0x5B09);
        assert_eq!(e.name.as_deref(), Some("LG HDR 4K"));
        assert_eq!(e.size_mm, Some((597.0, 336.0)));
    }

    #[test]
    fn falls_back_to_cm_when_timing_size_is_nonsense() {
        let mut b = sample();
        b[66] = 16;
        b[67] = 9;
        b[68] = 0;
        assert_eq!(parse(&b).unwrap().size_mm, Some((600.0, 340.0)));
    }

    #[test]
    fn whole_centimetre_sizes_are_rebuilt_from_the_diagonal() {
        // LG 27" reporting 600 x 340 mm; the real panel is 597.7 x 336.2 mm.
        let (w, h) = refine_rounded_size((600.0, 340.0), (2560.0, 1440.0));
        assert!((w - 597.7).abs() < 0.2 && (h - 336.2).abs() < 0.2, "{w} x {h}");
        // Portrait: sizes and pixels both already rotated.
        let (w, h) = refine_rounded_size((340.0, 600.0), (1080.0, 1920.0));
        assert!((w - 336.2).abs() < 0.2 && (h - 597.7).abs() < 0.2, "{w} x {h}");
    }

    #[test]
    fn exact_sizes_are_kept() {
        assert_eq!(refine_rounded_size((597.0, 336.0), (2560.0, 1440.0)), (597.0, 336.0));
    }

    #[test]
    fn rejects_non_edid() {
        assert!(parse(&[0u8; 128]).is_none());
    }
}
