# Accretion model specification

What the new Classic tier computes, for the crate that replaces `sg-classic`
(ADR 0006). Everything here comes from the four publications below. Implementers
work from this document and the papers, and must not read `crates/sg-classic`,
`reference/stargen-c` or `docs/classic-constants.md`.

## Sources

| Key | Publication | Used for |
|---|---|---|
| D | Dole, S. H. (1969), *Formation of Planetary Systems by Aggregation: A Computer Simulation*, RAND P-4226; also *Icarus* 13, 494 (1970) | The accretion model. Page numbers are the paper's own |
| F | Fogg, M. J. (1985), *Extra-Solar Planetary Systems: A Microcomputer Simulation*, *JBIS* 38, 501 | Other stellar masses, planet properties |
| H | Hart, M. H. (1978), *The Evolution of the Atmosphere of the Earth*, *Icarus* 33, 23 | Water vapour, ice, greenhouse |
| K | Kothari, D. S. (1936), *The Internal Constitution of the Planets*, *MNRAS* 96, 833 | Mass–radius relation of cold bodies |

Items marked **Gap** are not settled by the sources. The owner accepted all the
answers in the [gaps table](#gaps-and-answers) on 2026-10-30.

## 1. The cloud

Units: AU and solar masses. `r` is distance from the star.

- Dust density: `ρ₁(r) = A exp(−α r^(1/n))` (D p.14).
- Gas and dust together: `ρ₂ = K ρ₁`, where `K` is the gas/dust ratio (D p.14).
- All cloud particles share one eccentricity `W` (D p.15).
- Other stellar masses: `A ∝ (M/M☉)^(1/2)` (F Eq. 8).

Parameters:

| Symbol | Value | Source |
|---|---|---|
| `α`, `n` | 5, 3 | D p.15 |
| `A`, `K`, `W` | Set 3: 0.0015, 50, 0.20. Set 4: 0.0015, 50, 0.25 | D Table 2 |
| `m₀` | 10⁻¹⁵ M☉; the result does not depend on it | D p.22 |
| `Q` | 0.077 | D p.15 |
| `B` | 1.2 × 10⁻⁵ M☉ AU^(3/4) | D p.21, F Eq. 7 |
| Inner, outer bound | 0.3 AU, 50 AU | D p.17 |

## 2. Nuclei

Nuclei of mass `m₀` are injected one at a time, and each grows to completion
before the next (D p.15). Each is in the invariable plane, with:

- `a = 50 Y₁`, with `Y₁` uniform on 0–1 (D p.15; **Gap 1** for the inner bound).
- `ε = 1 − (1 − Y₂)^Q` (D p.15).

Random numbers come from the tier's own ChaCha stream, drawn in that order.

## 3. Growth

For orbit `a`, `ε`, with perihelion `r_p = a(1 − ε)` and aphelion
`r_a = a(1 + ε)`, and with `μ = m/(1 + m)` in units of the stellar mass:

- Gravitational reach: `x = r μ^(1/4)` (D p.17), so `x_p = r_p μ^(1/4)` and
  `x_a = r_a μ^(1/4)`.
- Swept band width (D p.20):
  `2aε + x_a + x_p + W(r_a + x_a)/(1 − W) + W(r_p − x_p)/(1 + W)`.
- Swept limits (D Fig. 6): inner `(r_p − x_p)/(1 + W)`, outer
  `(r_a + x_a)/(1 − W)`.
- Swept volume: `2π a × (band width) × (x_a + x_p)` (D p.20; the factor `a` is
  needed for his closed form below and is implied by "2π(bandwidth)").
- Swept mass in an untouched cloud, the closed form (D p.20):
  `m_{i+1} = 8π a³ ρ μ_i^(1/4) / (1 − W²) × (ε + μ_i^(1/4) + W + W ε μ_i^(1/4))`.
- Stop when the increase between iterations is below 10⁻⁴ of the mass (D p.21).

Critical mass and density of the accreted material:

- `m_c = B (r_p (L☉/L)^(1/2))^(−3/4)` (F Eq. 7; D p.21 is the case `L = L☉`).
- Below `m_c`, only dust: `ρ = ρ₁` (D p.21).
- Above it: `ρ = K ρ₁ / (1 + (m_c/m_i)^(1/2) (K − 1))` (D p.21).

A nucleus in a region with no dust cannot grow and is discarded (D p.23, a "dud").

## 4. Bands

Dole's program keeps three kinds of band (D p.23): type 0, dust and gas as
original; type 1, no dust, all the gas; type 2, no dust and some gas removed.
Sweeping turns type 0 into type 1, or type 2 above `m_c`. The closed form above
applies only to an untouched cloud: Dole says his program used more complicated
expressions for overlapping bands and does not give them (D p.20, note 2).
**Gaps 2 and 3.**

The run ends when no type 0 band remains between the bounds (D p.23).

## 5. Coalescence

Two planets coalesce when they come within `x` of each other (D p.22), checked
after growth has completed (D p.22). No mass is lost (D p.22). The result takes:

- `a₃ = (m₁ + m₂) / (m₁/a₁ + m₂/a₂)` (D p.23).
- `ε₃ = {1 − [(m₁ √a₁ (1−ε₁²)^½ + m₂ √a₂ (1−ε₂²)^½) / ((m₁+m₂) √a₃)]²}^½` (D p.23).

The merged body may keep growing (D p.22). **Gap 4** for the exact test.

## 6. The star

Used only where the tier needs them; see **Gap 5**.

- `L/L☉ = (M/M☉)^n` (F Eq. 1), with `n = 1.75(M/M☉ − 0.1) + 3.325` below 1 M☉
  and `n = 0.5(2 − M/M☉) + 4.4` from 1 M☉ (F Eqs 2, 3).
- Main-sequence lifetime `t_ms ≈ 10 (M/M☉)/(L/L☉)` Gyr (F Eq. 4).
- Age: uniform on 1–6 Gyr if `t_ms ≥ 6` Gyr, otherwise on 1 Gyr–`t_ms` (F p.502).
- Mean habitable radius `r_ecos = (L/L☉)^(1/2)` AU; inner edge `0.93 r_ecos`
  (F Eqs 5, 6).

## 7. Planets

**Gap 6** decides which of these the tier uses and which come from `sg-planet`.

Density and radius:

- Below `m_c`: `ρ = 5.5 (m/m⊕)^(1/8) (r_ecos/R)^(1/4)` g/cm³ (F Eq. 10).
- Cold bodies generally: K Eq. 23, as restated in F Eq. 9. Its maximum radius is
  at `M₀ ~ 1.04 × 10⁻³ (Z³/A²) M☉` (K Eq. 25), with
  `R_max ~ 1.12 × 10¹⁰ (Z^(2/3)/A)` cm (K Eq. 26), and
  `R/R_max = 2(M/M₀)^(1/3) / (1 + (M/M₀)^(2/3))` (K Eq. 26′). Fogg gives no
  values for `Z` and `A` (**Gap 7**).

Rotation:

- `ω = (j m / (k₂/2 · r²))^(1/2)` (F Eq. 12), `j = 1.46 × 10⁻¹⁹` cm²/s²g,
  `k₂ ≈ 0.33` terrestrial and `0.24` giant (F p.506).
- Tidal slowing relative to Earth, with `ω̇⊕ = −1.3 × 10⁻⁶` rad/s per Gyr
  (F Eq. 13). A despun planet with `e > 0.1` locks to a resonance from
  `(1 − e)/(1 + e)` (F Eq. 14).

Atmosphere and climate:

- Escape and thermal speed: `V_e = √(2gr)`, `V₀ = √(2kT/m_a)`; a gas is kept when
  `V_e/V₀ ≥ 5`. Exospheric temperature about 1000 °C, scaling as `r_p^(−1/2)`
  (F Eqs 15, 16, p.507).
- Volatile inventory `V_inv = q (m/m⊕)(M/M☉)⁻¹ ± 20%`, with `q = 100000` in zone
  1, `75000` in zone 2 and `250` in zone 3; in zone 1 at `R ≥ r_inner`,
  `V_inv/100` (F Eq. 17, p.507).
- Zones at 1 M☉: to about 4 AU, about 4 to 14–16 AU, beyond; they scale as
  `(L/L☉)^(1/2)` (F p.505).
- Surface pressure `P ≈ V_inv (r/r⊕)⁻² (g/g⊕)` mb (F Eq. 18).
- `T_eff = (r_ecos/R)^(1/2) ((1 − A)/0.7)^(1/4) × 255 K` (F Eq. 19).
- `ΔT = [(1 + ¾τ)^(1/4) − 1] T_eff F_conv`, `F_conv = 0.43` (H Eq. 20, F Eq. 20).
  Each gas's optical depth goes as the square root of its quantity (F p.509).
- Boiling point `(ln(P/1000)/(−5050.5) + 1/373)⁻¹ − 273` °C (F Eq. 21).
- Hydrosphere `F_hyd = (0.75 V_inv/1000)(r/r⊕)⁻²` (F Eq. 22).
- Water vapour `Q₁ exp[Q₂(T_surf − 288)]`, `Q₁ = 1.258 × 10¹⁹` g,
  `Q₂ = 0.0698` K⁻¹ (H Eq. 3, F Eq. 23). Cloud cover is proportional to it.
- Ice `F_ice = ((328 − T_surf)/70)⁵`, at most `1.5 F_hyd` (H p.28, F Eq. 24).
- Albedo: gas giant 0.4, airless rock 0.07, airless ice 0.4–0.7; cloud 0.52, rock
  0.15, ocean 0.04, ice 0.7 (F Table 3, H Table I). Feedback runs for planets
  between `r_inner` and `4 r_ecos` until `T_surf` changes by under 1 °C (F p.509).

## 8. Checks

Each comes from the sources and becomes a test.

Direct values:

| Check | Expected | Source |
|---|---|---|
| Cloud mass, Set 3 | 0.0584 M☉ × cos(π/2 − θ_max) | D p.14 |
| Peak of `r³ρ₁` | 5.83 AU | D p.14 |
| `m_c` at 1 AU, 1 L☉ | about 4 Earth masses | D p.21 |
| Median `ε` | `1 − 0.5^0.077` = 0.0520 | follows from D p.15 |
| `L` at 0.6 and 1.3 M☉ | 0.12 and 3.48 L☉ | F Table 1 |
| `n` continuous at 1 M☉ | 4.9 from both sides | F Eqs 2, 3 |
| Density, 1.8 m⊕ at 0.965 AU | 5.98 g/cm³ | F Fig. 11 |
| Density, 4.9 × 10²³ kg at 0.34 AU | 5.26 g/cm³ | F Fig. 10 |
| Boiling point at 1555 mb | 112.58 °C | F Fig. 11 |
| Earth's initial rotation | about 15 h | F Eq. 12, p.506 |
| `R` at `M₀` | `R_max` | K Eq. 26′ |

Growth, from Dole's run 41 of Set 3 (`A` 0.0015, `K` 50, `W` 0.20; D pp.25–26).
These were checked against this document with a throwaway script before it was
written:

| Nucleus | `a`, `ε` | Dole's limits (AU) | This document |
|---|---|---|---|
| 1, first, untouched cloud | 22.8, 0.149 | 14.4–36.5 | 14.39–36.36 |
| 2 | 43.1, 0.112 | outer 61.6 | outer 61.69 |
| 4 | 9.29, 0.007 | 6.45–13.6 | 6.47–13.55 |
| 29 | 0.421, 0.269 | 0.248–0.690 | 0.248–0.690 |
| 6 | 2.02, 0.092 | 1.14–2.97 | 1.41–2.97 |

For nucleus 6 the printed 1.14 is probably a misprint of 1.41: nucleus 8 is
then said to sweep the remaining dust between 1.28 and 5.91 AU (D p.25), which
could not exist inside a band already swept to 1.14. Nucleus 8 enters swept
bands, so the untouched-cloud form rightly does not reproduce it (1.20–6.19).

Population, over 40 runs at each setting (D Tables 3, 4):

| Statistic | Set 4 | Set 3 |
|---|---|---|
| Planets per system: mean, range | 9.2, 7–11 | 10.1, 7–12 |
| Total mass, 10⁻³ M☉: mean, range | 1.56, 0.43–3.04 | 1.16, 0.58–1.92 |
| Largest planet, Earth masses: mean, range | 305, 63–979 | 258, 90–594 |
| Spacing ratio: mean, range | 1.84, 1.22–3.37 | 1.73, 1.17–4.09 |

Dole's spacing ratios exclude bodies below 10⁻⁷ M☉ (D p.38). Mean planets per
mass bin, from 10⁻¹⁰ to 10⁻² M☉ by decades (D Table 4): Set 4 0.03, 0.08, 0.40,
2.9, 2.8, 1.1, 1.7, 0.65; Set 3 0.03, 0.03, 0.53, 3.2, 3.4, 1.3, 1.9, 0.33.

Dole's runs are 40 systems, so a test must compare his means with tolerances
that follow from that sample, not from a much larger one.

Result of the implementation, 2000 runs at each setting:

| Statistic | Set 4 | Set 3 |
|---|---|---|
| Planets per system: mean | 10.41 | 11.39 |
| Total mass, 10⁻³ M☉: mean | 1.43 | 1.08 |
| Largest planet, Earth masses: mean | 363 | 263 |
| Spacing ratio: mean | 1.76 | 1.67 |

Mass and spacing agree within the error of Dole's means. **Planet count does
not:** it is about 1.2 above his mean, in bodies of 10⁻⁷–10⁻⁵ M☉, though inside
his range. His Table 4 also sums to about 0.5 more than his Table 3 mean, so part
of the difference may be how he counted; the cause is not established.

## Gaps and answers

| # | Gap | Answer |
|---|---|---|
| 1 | `a = 50Y` can fall below the 0.3 AU bound, and Dole does not say what then happens. Figures show no planet well inside it | Redraw until `a ≥ 0.3`, keeping `a = 50Y` above it |
| 2 | Mass swept from a partly cleared annulus (D p.20, note 2) | Sum over bands: each band's share of the swept width × that band's density, keeping Dole's density-at-`a`. Reduces to his closed form in an untouched cloud |
| 3 | How much gas a type 2 band keeps | Each band holds a remaining gas fraction, reduced by what is accreted. **Changed in implementation:** gas is gathered only where dust is swept, so bands without dust give nothing. The accepted answer let merged planets take the same gas repeatedly, doubling total mass; Dole's text (pp.21, 23, 25) supports the change |
| 4 | The coalescence test. Dole's Event 6 has one planet's `x` limits overlap the other's `ε` limits | Coalesce when either planet's `x` limits overlap the other's `ε` limits; merge in order of injection |
| 5 | Fogg covers 0.6–1.3 M☉ and single stars, and leaves the bounds unscaled. The tier covers more | Take `L`, age and lifetime from `sg-stellar` for every star, with F Eqs 1–4 kept only as tests; extrapolate `A` and `m_c` beyond Fogg's range; say so on the card. One path for all stars. Multiple stars narrow the bounds as the disk limits require |
| 6 | Fogg's atmospheres are seven hand-picked classes without published selection rules | Use `sg-planet` for radius, escape and class wherever it applies. Use Fogg's relations for what it lacks, and drop his life and habitability rules |
| 7 | Fogg gives no `Z` and `A`, and his "Eq. 10" reference on p.505 means Eq. 9 | Follows from 6: `sg-planet` for giant radii; K only as a test |
| 8 | Fogg's "ideal" values are not stated | Set 4 as the default, the closer to the Solar System in Dole's Table 3; Set 3 selectable |

## What was seen of the old code

This document was written from the papers. Its author had seen the crate's
public type names, module comments, constants file header and the first rows of
its constants table while surveying the repository, and read no function bodies
of `sg-classic` or any of the C sources. It is not a strict clean-room record:
see ADR 0006.
