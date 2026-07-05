//! ISO paper sizes and sheet orientation for window plotting.

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PaperSize {
    A4,
    A3,
    A2,
    A1,
    A0,
}

impl PaperSize {
    pub const ALL: [PaperSize; 5] = [
        PaperSize::A4,
        PaperSize::A3,
        PaperSize::A2,
        PaperSize::A1,
        PaperSize::A0,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::A3 => "A3",
            PaperSize::A2 => "A2",
            PaperSize::A1 => "A1",
            PaperSize::A0 => "A0",
        }
    }

    /// Portrait dimensions in mm (width, height); width < height.
    pub fn dimensions_mm(self) -> (f64, f64) {
        match self {
            PaperSize::A4 => (210.0, 297.0),
            PaperSize::A3 => (297.0, 420.0),
            PaperSize::A2 => (420.0, 594.0),
            PaperSize::A1 => (594.0, 841.0),
            PaperSize::A0 => (841.0, 1189.0),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Sheet dimensions in mm for the given size and orientation.
pub fn sheet_mm(size: PaperSize, o: Orientation) -> (f64, f64) {
    let (w, h) = size.dimensions_mm();
    match o {
        Orientation::Portrait => (w, h),
        Orientation::Landscape => (h, w),
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PlotScale {
    /// Scale so the window fills the sheet minus a 5% margin.
    Fit,
    /// Exact scale factor (mm per drawing unit).
    Ratio(f64),
}

/// Map a world-space window onto a sheet. Returns (scale, offset_x, offset_y):
/// world (x, y) -> sheet mm ((x - window_min.0) * scale + offset_x, (y - window_min.1) * scale + offset_y).
/// The scaled window is centered on the sheet.
pub fn window_to_sheet(
    window_wh: (f64, f64),
    window_min: (f64, f64),
    sheet_mm: (f64, f64),
    scale: PlotScale,
) -> (f64, f64, f64) {
    let (ww, wh) = (window_wh.0.max(1e-9), window_wh.1.max(1e-9));
    let scale = match scale {
        PlotScale::Ratio(r) => r.max(1e-9),
        PlotScale::Fit => {
            const MARGIN: f64 = 1.05;
            let sx = (sheet_mm.0 / MARGIN) / ww;
            let sy = (sheet_mm.1 / MARGIN) / wh;
            sx.min(sy)
        }
    };
    let scaled_w = ww * scale;
    let scaled_h = wh * scale;
    let offset_x = (sheet_mm.0 - scaled_w) / 2.0;
    let offset_y = (sheet_mm.1 - scaled_h) / 2.0;
    let _ = window_min; // offset already places the window min at (offset_x, offset_y)
    (scale, offset_x, offset_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dimensions_and_orientation() {
        assert_eq!(PaperSize::A4.dimensions_mm(), (210.0, 297.0));
        assert_eq!(PaperSize::A0.dimensions_mm(), (841.0, 1189.0));
        assert_eq!(PaperSize::ALL.len(), 5);
        assert_eq!(PaperSize::A3.label(), "A3");
        // Portrait keeps (w,h); landscape swaps.
        assert_eq!(sheet_mm(PaperSize::A4, Orientation::Portrait), (210.0, 297.0));
        assert_eq!(sheet_mm(PaperSize::A4, Orientation::Landscape), (297.0, 210.0));
    }

    #[test]
    fn fit_centers_and_scales_within_margin() {
        // 100×100 window onto a 210×297 sheet, Fit. Limiting axis is width:
        // usable = 210/1.05 = 200; scale = 200/100 = 2.0.
        let (s, ox, oy) = window_to_sheet((100.0, 100.0), (0.0, 0.0), (210.0, 297.0), PlotScale::Fit);
        assert!((s - 2.0).abs() < 1e-9, "scale {s}");
        // Window is 100*2 = 200 wide/tall; centered on 210×297.
        assert!((ox - (210.0 - 200.0) / 2.0).abs() < 1e-9, "ox {ox}");
        assert!((oy - (297.0 - 200.0) / 2.0).abs() < 1e-9, "oy {oy}");
    }

    #[test]
    fn ratio_applies_exact_scale_and_offsets_window_min() {
        // Ratio(0.5): scale is exactly 0.5; a window starting at (10,20)
        // maps its min corner to the offset so (10,20) -> (ox,oy).
        let (s, ox, oy) = window_to_sheet((100.0, 80.0), (10.0, 20.0), (210.0, 297.0), PlotScale::Ratio(0.5));
        assert!((s - 0.5).abs() < 1e-9);
        // Centered: sheet_center - scaled_window_center, then min maps to offset.
        // scaled window = 50×40; centered box origin = ((210-50)/2,(297-40)/2).
        assert!((ox - (210.0 - 50.0) / 2.0).abs() < 1e-9, "ox {ox}");
        assert!((oy - (297.0 - 40.0) / 2.0).abs() < 1e-9, "oy {oy}");
    }
}
