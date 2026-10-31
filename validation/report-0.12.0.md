# Validation report: Standard tier, model 0.12.0

3000 Sun-like single stars (seeds 1–3000), with metallicity, disk mass, disk lifetime and age drawn from the model's distributions. 56672 planets in all, 18.9 per star.

**7 of 7 metrics inside tolerance.**

The model's planets are counted as generated, with no simulation of what a survey would detect. Tolerances are wide on purpose: they test whether the model is in the right regime, not whether it fits.

| Metric | Model | Observed | Tolerance | Result |
|---|---|---|---|---|
| Stars with a planet of 1–4 R⊕ inside 100 days | 18.33% ± 0.71 | about 30% | 15–60% | pass |
| Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days) | 1.00% ± 0.18 | 0.4–1.2% | 0.2–3% | pass |
| Stars with a giant (≥ 0.3 M_J) at 10–2000 days | 10.93% ± 0.57 | about 10% | 4–25% | pass |
| Stars with a Jupiter analogue (≥ 0.3 M_J at 3–7 AU) | 2.90% ± 0.31 | about 6% | 2–15% | pass |
| Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1 | 4.39× ± 0.67 | 3 or more | ≥ 2× | pass |
| Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side | 0.80 | about 0.5 | 0–0.8 | pass |
| Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days | 1.75 | about 1 | 0.5–2 | pass |

For reference, between 1.25 and 3 R⊕ the model has fewest close-in planets at 2.75–3 R⊕, where their density is 0.33 of the highest in that range. The observed valley is centred near 1.8 R⊕.

Not scored: the model has 3.7 planets smaller than the Earth (0.5–1 R⊕) inside 100 days per star, formed where they are from local planetesimals. Surveys are too incomplete at these sizes to give a firm number; it is probably too many (the Solar System has one, Mercury).

Not scored: the model has 0.04 planets of 1.25–1.75 R⊕ inside 100 days per star, against 0.13 of 1–1.25 R⊕. Surveys find the lower of the two size peaks at about 1.3 R⊕ (Fulton et al. 2017), so the model's rocky planets are too small: it makes few bare cores of 2–5 Earth masses.

Sources:

- Stars with a planet of 1–4 R⊕ inside 100 days: Zhu et al. 2018, ApJ 860, 101 (30 ± 3% of Sun-like stars).
- Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days): Howard et al. 2012, ApJS 201, 15; Wright et al. 2012, ApJ 753, 160.
- Stars with a giant (≥ 0.3 M_J) at 10–2000 days: Cumming et al. 2008, PASP 120, 531 (10.5% for 0.3–10 M_J inside 2000 days).
- Stars with a Jupiter analogue (≥ 0.3 M_J at 3–7 AU): Wittenmyer et al. 2016, ApJ 819, 28 (6.2 +2.8/−1.6% of Sun-like stars).
- Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1: Fischer & Valenti 2005, ApJ 622, 1102 (rate ∝ 10^(2[Fe/H])).
- Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side: Fulton et al. 2017, AJ 154, 109 (a deficit of about a factor of two).
- Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days: Fulton et al. 2017, AJ 154, 109 (two peaks of similar height).

## Speed

Mean 145.0 ms per system, slowest 438.5 ms, on the machine that produced this report (one thread). The target is under 2 s on a mid-range phone; that has not been measured.

## Distributions

### Radii of planets inside 100 days (R⊕)

```
    0.50 – 0.70       2.775  ##################################################
    0.70 – 1.00       0.898  ################
    1.00 – 1.25       0.127  ##
    1.25 – 1.50       0.023  
    1.50 – 1.75       0.016  
    1.75 – 2.00       0.023  
    2.00 – 2.50       0.049  
    2.50 – 3.00       0.021  
    3.00 – 4.00       0.002  
    4.00 – 6.00       0.000  
    6.00 – 10.00      0.064  #
   10.00 – 16.00      0.053  
```

Counts are planets per star.

### Orbital periods of all planets (days)

```
    1.00 – 3.00       0.114  #
    3.00 – 10.00      1.049  ###########
   10.00 – 30.00      3.519  ######################################
   30.00 – 100.00     4.618  ##################################################
  100.00 – 300.00     3.742  ########################################
  300.00 – 1000.00    3.446  #####################################
 1000.00 – 3000.00    1.675  ##################
 3000.00 – 10000.00   0.548  #####
10000.00 – 100000.00   0.180  #
100000.00 – 1000000.00   0.000  
```

Counts are planets per star.

### Masses of all planets (M⊕)

```
    0.01 – 0.10       6.831  ##################################################
    0.10 – 0.30       6.162  #############################################
    0.30 – 1.00       3.049  ######################
    1.00 – 3.00       1.343  #########
    3.00 – 10.00      0.814  #####
   10.00 – 30.00      0.327  ##
   30.00 – 100.00     0.154  #
  100.00 – 300.00     0.135  
  300.00 – 1000.00    0.061  
 1000.00 – 10000.00   0.014  
```

Counts are planets per star.

### Distances of giant planets (≥ 0.3 M_J) from their stars (AU, for 1 M☉)

```
    0.01 – 0.10       0.011  #########
    0.10 – 0.30       0.026  ####################
    0.30 – 1.00       0.063  ##################################################
    1.00 – 2.00       0.027  #####################
    2.00 – 3.00       0.014  ###########
    3.00 – 5.00       0.015  ############
    5.00 – 7.00       0.014  ##########
    7.00 – 10.00      0.013  ##########
   10.00 – 20.00      0.029  #######################
   20.00 – 100.00     0.005  ###
```

Counts are planets per star.
