//! Proxy entity graphics — the cached vector preview an application stores
//! alongside a custom entity so viewers without its object enabler can still
//! draw something. AutoCAD falls back to exactly this; so do we.
//!
//! No reference decoder exists to copy: LibreDWG keeps the blob raw and points
//! at the spec ("see par 29"), ACadSharp likewise stores `ProxyGraphics` bytes
//! and never parses them. The full record grammar is therefore unverified here,
//! and guessing at it from a sample is how you end up misreading every file.
//!
//! So this decodes exactly one shape — a single polyline record — and only when
//! the blob matches it byte-for-byte. Anything else returns `None` and draws
//! nothing, which is what happened before this existed: no regression, no
//! invented geometry.

/// A closed/open polyline lifted from a proxy-graphics blob (world coords).
pub struct ProxyPolyline {
    pub points: Vec<[f64; 3]>,
}

/// Record type for a polyline entry. The only one accepted.
const ENTRY_POLYLINE: u32 = 6;

/// Layout accepted (little-endian), validated in full before anything is
/// emitted:
///
/// ```text
/// u32 total_size      == blob.len()
/// u32 entry_count     == 1
/// u32 payload_size    == blob.len() - 8
/// u32 entry_type      == 6 (polyline)
/// u32 point_count
/// [f64; 3] * point_count
/// ```
///
/// Verified against an Autodesk Raster Design embedded raster image, whose
/// 140-byte preview decodes to its image frame with 0 bytes left over.
pub fn decode_polyline(blob: &[u8]) -> Option<ProxyPolyline> {
    let u32_at = |o: usize| -> Option<u32> {
        blob.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };

    if blob.len() < 20 || u32_at(0)? as usize != blob.len() || u32_at(4)? != 1 {
        return None;
    }
    if u32_at(8)? as usize != blob.len() - 8 || u32_at(12)? != ENTRY_POLYLINE {
        return None;
    }
    let n = u32_at(16)? as usize;
    // The points must account for every remaining byte — a short read would
    // mean the record carries something this decoder does not model.
    if n < 2 || 20usize.checked_add(n.checked_mul(24)?)? != blob.len() {
        return None;
    }

    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let o = 20 + i * 24;
        let f = |k: usize| -> f64 {
            let b = &blob[o + k * 8..o + k * 8 + 8];
            f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        };
        let (x, y, z) = (f(0), f(1), f(2));
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return None;
        }
        points.push([x, y, z]);
    }
    Some(ProxyPolyline { points })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real 140-byte preview of an Autodesk Raster Design embedded raster
    /// image: its frame, 1192.149 x 877.824, closed back to the origin.
    #[test]
    fn decodes_the_raster_design_image_frame() {
        let hex = "8C00000001000000840000000600000005000000\
                   000000000000000000000000000000000000000000000000\
                   DEF97E6A1C422E3D3DDF4F8D976E8B400000000000000000\
                   6BBC749398A092403DDF4F8D976E8B400000000000000000\
                   6BBC7493 98A09240 0000000000000000 0000000000000000\
                   000000000000000000000000000000000000000000000000";
        let blob: Vec<u8> = hex
            .split_whitespace()
            .collect::<String>()
            .as_bytes()
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(blob.len(), 140);

        let p = decode_polyline(&blob).expect("frame decodes");
        assert_eq!(p.points.len(), 5);
        assert!((p.points[2][0] - 1192.1490).abs() < 1e-3);
        assert!((p.points[2][1] - 877.8240).abs() < 1e-3);
        // Closed: last point returns to the first.
        assert_eq!(p.points[0], p.points[4]);
    }

    #[test]
    fn rejects_anything_it_does_not_model() {
        assert!(decode_polyline(&[]).is_none());
        assert!(decode_polyline(&[0u8; 140]).is_none()); // size field wrong
        let mut b = vec![0u8; 140];
        b[0] = 140; // right size, wrong everything else
        assert!(decode_polyline(&b).is_none());
    }
}
