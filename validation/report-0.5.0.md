# Validation report: Standard tier, model 0.5.0

3000 Sun-like single stars (seeds 1–3000), with metallicity, disk mass, disk lifetime and age drawn from the model's distributions. 60738 planets in all, 20.2 per star.

**6 of 6 metrics inside tolerance.**

The model's planets are counted as generated, with no simulation of what a survey would detect. Tolerances are wide on purpose: they test whether the model is in the right regime, not whether it fits.

| Metric | Model | Observed | Tolerance | Result |
|---|---|---|---|---|
| Stars with a planet of 1–4 R⊕ inside 100 days | 21.43% ± 0.75 | about 30% | 15–60% | pass |
| Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days) | 1.33% ± 0.21 | 0.4–1.2% | 0.2–3% | pass |
| Stars with a giant (≥ 0.3 M_J) at 10–2000 days | 11.53% ± 0.58 | about 10% | 4–25% | pass |
| Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1 | 4.48× ± 0.66 | 3 or more | ≥ 2× | pass |
| Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side | 0.66 | about 0.5 | 0–0.8 | pass |
| Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days | 0.59 | about 1 | 0.5–2 | pass |

For reference, between 1.25 and 3 R⊕ the model has fewest close-in planets at 1.25–1.5 R⊕, where their density is 0.17 of the highest in that range. The observed valley is centred near 1.8 R⊕.

Not scored: the model has 4.8 planets smaller than the Earth (0.5–1 R⊕) inside 100 days per star. Surveys are too incomplete at these sizes to give a firm number, but this is probably several times too many.

Sources:

- Stars with a planet of 1–4 R⊕ inside 100 days: Zhu et al. 2018, ApJ 860, 101 (30 ± 3% of Sun-like stars).
- Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days): Howard et al. 2012, ApJS 201, 15; Wright et al. 2012, ApJ 753, 160.
- Stars with a giant (≥ 0.3 M_J) at 10–2000 days: Cumming et al. 2008, PASP 120, 531 (10.5% for 0.3–10 M_J inside 2000 days).
- Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1: Fischer & Valenti 2005, ApJ 622, 1102 (rate ∝ 10^(2[Fe/H])).
- Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side: Fulton et al. 2017, AJ 154, 109 (a deficit of about a factor of two).
- Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days: Fulton et al. 2017, AJ 154, 109 (two peaks of similar height).

## Speed

Mean 75.8 ms per system, slowest 252.0 ms, on the machine that produced this report (one thread). The target is under 2 s on a mid-range phone; that has not been measured.

## Distributions

### Radii of planets inside 100 days (R⊕)

```
    0.50 – 0.70       3.686  ##################################################
    0.70 – 1.00       1.156  ###############
    1.00 – 1.25       0.082  #
    1.25 – 1.50       0.017  
    1.50 – 1.75       0.019  
    1.75 – 2.00       0.027  
    2.00 – 2.50       0.083  #
    2.50 – 3.00       0.078  #
    3.00 – 4.00       0.012  
    4.00 – 6.00       0.000  
    6.00 – 10.00      0.085  #
   10.00 – 16.00      0.069  
```

Counts are planets per star.

### Orbital periods of all planets (days)

```
    1.00 – 3.00       0.111  #
    3.00 – 10.00      1.513  ###############
   10.00 – 30.00      3.891  ########################################
   30.00 – 100.00     4.751  ##################################################
  100.00 – 300.00     3.490  ####################################
  300.00 – 1000.00    3.506  ####################################
 1000.00 – 3000.00    2.588  ###########################
 3000.00 – 10000.00   0.391  ####
10000.00 – 100000.00   0.004  
100000.00 – 1000000.00   0.000  
```

Counts are planets per star.

### Masses of all planets (M⊕)

```
    0.01 – 0.10       5.446  ############################################
    0.10 – 0.30       6.067  ##################################################
    0.30 – 1.00       4.345  ###################################
    1.00 – 3.00       2.875  #######################
    3.00 – 10.00      1.062  ########
   10.00 – 30.00      0.215  #
   30.00 – 100.00     0.095  
  100.00 – 300.00     0.071  
  300.00 – 1000.00    0.058  
 1000.00 – 10000.00   0.012  
```

Counts are planets per star.
