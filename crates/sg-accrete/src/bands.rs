//! The state of the cloud: contiguous bands that have or have not lost their
//! dust and part of their gas.
//!
//! Dole (1969, p.23) names three kinds of band. His formulas for overlaps are
//! not published (p.20, note 2), so what happens to a partly cleared annulus is
//! ours (spec gaps 2 and 3): a body takes from each band in proportion to its
//! share of the swept width, and only where dust remains. The gas fraction left
//! in each band is bookkeeping for his type 1 and type 2 bands; it feeds no
//! growth, since gas comes only along with dust.

/// One band of the cloud.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub inner_au: f64,
    pub outer_au: f64,
    /// Whether it still holds its dust.
    pub dust: bool,
    /// Fraction of the original gas that remains, 0–1.
    pub gas_fraction: f64,
}

/// What a swept interval holds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Contents {
    /// Width with dust, AU.
    pub dust_width_au: f64,
    /// Width weighted by the remaining gas fraction, AU.
    pub gas_width_au: f64,
}

/// The whole cloud, ordered outward.
#[derive(Debug, Clone, PartialEq)]
pub struct Cloud {
    bands: Vec<Band>,
}

impl Cloud {
    /// An untouched cloud between the material limits.
    pub fn new(inner_au: f64, outer_au: f64) -> Self {
        Self {
            bands: vec![Band {
                inner_au,
                outer_au,
                dust: true,
                gas_fraction: 1.0,
            }],
        }
    }

    /// The bands, ordered outward.
    pub fn bands(&self) -> &[Band] {
        &self.bands
    }

    /// What lies between two radii.
    pub fn contents(&self, lo_au: f64, hi_au: f64) -> Contents {
        let mut c = Contents::default();
        for b in &self.bands {
            let width = (hi_au.min(b.outer_au) - lo_au.max(b.inner_au)).max(0.0);
            if b.dust {
                c.dust_width_au += width;
            }
            c.gas_width_au += width * b.gas_fraction;
        }
        c
    }

    /// Whether any dust remains between two radii.
    pub fn has_dust(&self, lo_au: f64, hi_au: f64) -> bool {
        self.contents(lo_au, hi_au).dust_width_au > 0.0
    }

    /// Clears the dust between two radii and keeps `gas_kept` of the gas there.
    pub fn sweep(&mut self, lo_au: f64, hi_au: f64, gas_kept: f64) {
        let mut out = Vec::with_capacity(self.bands.len() + 2);
        for b in &self.bands {
            let (from, to) = (lo_au.max(b.inner_au), hi_au.min(b.outer_au));
            if from >= to {
                out.push(*b);
                continue;
            }
            if b.inner_au < from {
                out.push(Band {
                    outer_au: from,
                    ..*b
                });
            }
            out.push(Band {
                inner_au: from,
                outer_au: to,
                dust: false,
                gas_fraction: b.gas_fraction * gas_kept,
            });
            if to < b.outer_au {
                out.push(Band { inner_au: to, ..*b });
            }
        }
        out.dedup_by(|next, prev| {
            let same = prev.dust == next.dust && prev.gas_fraction == next.gas_fraction;
            if same {
                prev.outer_au = next.outer_au;
            }
            same
        });
        self.bands = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeping_splits_and_reports() {
        let mut c = Cloud::new(0.0, 10.0);
        c.sweep(2.0, 4.0, 0.5);
        assert_eq!(c.bands().len(), 3);
        let inside = c.contents(2.0, 4.0);
        assert_eq!(inside.dust_width_au, 0.0);
        assert_eq!(inside.gas_width_au, 1.0);
        let across = c.contents(3.0, 5.0);
        assert_eq!(across.dust_width_au, 1.0);
        assert_eq!(across.gas_width_au, 1.5);
    }

    #[test]
    fn untouched_neighbours_merge_back() {
        let mut c = Cloud::new(0.0, 10.0);
        c.sweep(2.0, 4.0, 1.0);
        c.sweep(4.0, 6.0, 1.0);
        assert_eq!(c.bands().len(), 3);
        assert_eq!(c.bands()[1].outer_au, 6.0);
    }

    #[test]
    fn sweeping_beyond_the_cloud_changes_nothing_outside() {
        let mut c = Cloud::new(1.0, 2.0);
        c.sweep(0.0, 1.5, 0.0);
        assert!(!c.has_dust(1.0, 1.5));
        assert!(c.has_dust(1.5, 2.0));
        assert_eq!(c.bands()[0].inner_au, 1.0);
    }
}
