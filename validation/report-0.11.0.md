# Validation report: Standard tier, model 0.11.0

3000 Sun-like single stars (seeds 1–3000), with metallicity, disk mass, disk lifetime and age drawn from the model's distributions. 57922 planets in all, 19.3 per star.

**6 of 7 metrics inside tolerance.**

The model's planets are counted as generated, with no simulation of what a survey would detect. Tolerances are wide on purpose: they test whether the model is in the right regime, not whether it fits.

| Metric | Model | Observed | Tolerance | Result |
|---|---|---|---|---|
| Stars with a planet of 1–4 R⊕ inside 100 days | 20.53% ± 0.74 | about 30% | 15–60% | pass |
| Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days) | 0.57% ± 0.14 | 0.4–1.2% | 0.2–3% | pass |
| Stars with a giant (≥ 0.3 M_J) at 10–2000 days | 8.13% ± 0.50 | about 10% | 4–25% | pass |
| Stars with a Jupiter analogue (≥ 0.3 M_J at 3–7 AU) | 1.43% ± 0.22 | about 6% | 2–15% | **fail** |
| Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1 | 5.57× ± 1.08 | 3 or more | ≥ 2× | pass |
| Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side | 0.71 | about 0.5 | 0–0.8 | pass |
| Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days | 1.84 | about 1 | 0.5–2 | pass |

For reference, between 1.25 and 3 R⊕ the model has fewest close-in planets at 2.75–3 R⊕, where their density is 0.29 of the highest in that range. The observed valley is centred near 1.8 R⊕.

Not scored: the model has 4.1 planets smaller than the Earth (0.5–1 R⊕) inside 100 days per star, formed where they are from local planetesimals. Surveys are too incomplete at these sizes to give a firm number; it is probably too many (the Solar System has one, Mercury).

Not scored: the model has 0.04 planets of 1.25–1.75 R⊕ inside 100 days per star, against 0.18 of 1–1.25 R⊕. Surveys find the lower of the two size peaks at about 1.3 R⊕ (Fulton et al. 2017), so the model's rocky planets are too small: it makes few bare cores of 2–5 Earth masses.

Sources:

- Stars with a planet of 1–4 R⊕ inside 100 days: Zhu et al. 2018, ApJ 860, 101 (30 ± 3% of Sun-like stars).
- Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days): Howard et al. 2012, ApJS 201, 15; Wright et al. 2012, ApJ 753, 160.
- Stars with a giant (≥ 0.3 M_J) at 10–2000 days: Cumming et al. 2008, PASP 120, 531 (10.5% for 0.3–10 M_J inside 2000 days).
- Stars with a Jupiter analogue (≥ 0.3 M_J at 3–7 AU): Wittenmyer et al. 2016, ApJ 819, 28 (6.2 +2.8/−1.6% of Sun-like stars).
- Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1: Fischer & Valenti 2005, ApJ 622, 1102 (rate ∝ 10^(2[Fe/H])).
- Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side: Fulton et al. 2017, AJ 154, 109 (a deficit of about a factor of two).
- Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days: Fulton et al. 2017, AJ 154, 109 (two peaks of similar height).

## Speed

Mean 134.4 ms per system, slowest 408.2 ms, on the machine that produced this report (one thread). The target is under 2 s on a mid-range phone; that has not been measured.

## Distributions

### Radii of planets inside 100 days (R⊕)

```
    0.50 – 0.70       3.109  ##################################################
    0.70 – 1.00       1.035  ################
    1.00 – 1.25       0.177  ##
    1.25 – 1.50       0.024  
    1.50 – 1.75       0.018  
    1.75 – 2.00       0.028  
    2.00 – 2.50       0.062  
    2.50 – 3.00       0.028  
    3.00 – 4.00       0.002  
    4.00 – 6.00       0.000  
    6.00 – 10.00      0.063  #
   10.00 – 16.00      0.040  
```

Counts are planets per star.

### Orbital periods of all planets (days)

```
    1.00 – 3.00       0.107  #
    3.00 – 10.00      1.017  ##########
   10.00 – 30.00      3.534  #####################################
   30.00 – 100.00     4.725  ##################################################
  100.00 – 300.00     3.933  #########################################
  300.00 – 1000.00    3.626  ######################################
 1000.00 – 3000.00    1.715  ##################
 3000.00 – 10000.00   0.509  #####
10000.00 – 100000.00   0.140  #
100000.00 – 1000000.00   0.000  
```

Counts are planets per star.

### Masses of all planets (M⊕)

```
    0.01 – 0.10       6.188  ##############################################
    0.10 – 0.30       6.600  ##################################################
    0.30 – 1.00       3.552  ##########################
    1.00 – 3.00       1.760  #############
    3.00 – 10.00      0.811  ######
   10.00 – 30.00      0.183  #
   30.00 – 100.00     0.083  
  100.00 – 300.00     0.079  
  300.00 – 1000.00    0.039  
 1000.00 – 10000.00   0.013  
```

Counts are planets per star.

### Distances of giant planets (≥ 0.3 M_J) from their stars (AU, for 1 M☉)

```
    0.01 – 0.10       0.006  #####
    0.10 – 0.30       0.017  ###############
    0.30 – 1.00       0.056  ##################################################
    1.00 – 2.00       0.017  ##############
    2.00 – 3.00       0.010  ########
    3.00 – 5.00       0.008  #######
    5.00 – 7.00       0.006  #####
    7.00 – 10.00      0.005  ####
   10.00 – 20.00      0.009  #######
   20.00 – 100.00     0.001  
```

Counts are planets per star.
