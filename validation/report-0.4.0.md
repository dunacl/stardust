# Validation report: Standard tier, model 0.4.0

3000 Sun-like single stars (seeds 1–3000), with metallicity, disk mass, disk lifetime and age drawn from the model's distributions. 23638 planets in all, 7.9 per star.

**3 of 5 metrics inside tolerance.**

The model's planets are counted as generated, with no simulation of what a survey would detect. Tolerances are wide on purpose: they test whether the model is in the right regime, not whether it fits.

| Metric | Model | Observed | Tolerance | Result |
|---|---|---|---|---|
| Stars with a planet of 1–4 R⊕ inside 100 days | 31.43% ± 0.85 | about 30% | 15–60% | pass |
| Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days) | 1.97% ± 0.25 | 0.4–1.2% | 0.2–3% | pass |
| Stars with a giant (≥ 0.3 M_J) at 10–2000 days | 6.17% ± 0.44 | about 10% | 4–25% | pass |
| Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1 | 1.95× ± 0.34 | 3 or more | ≥ 2× | **fail** |
| Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side | 1.73 | about 0.5 | 0–0.8 | **fail** |

For reference, between 1.25 and 3 R⊕ the model has fewest close-in planets at 2.75–3 R⊕, where their density is 0.37 of the highest in that range. The observed valley is centred near 1.8 R⊕.

Sources:

- Stars with a planet of 1–4 R⊕ inside 100 days: Zhu et al. 2018, ApJ 860, 101 (30 ± 3% of Sun-like stars).
- Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days): Howard et al. 2012, ApJS 201, 15; Wright et al. 2012, ApJ 753, 160.
- Stars with a giant (≥ 0.3 M_J) at 10–2000 days: Cumming et al. 2008, PASP 120, 531 (10.5% for 0.3–10 M_J inside 2000 days).
- Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1: Fischer & Valenti 2005, ApJ 622, 1102 (rate ∝ 10^(2[Fe/H])).
- Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side: Fulton et al. 2017, AJ 154, 109 (a deficit of about a factor of two).

## Speed

Mean 15.2 ms per system, slowest 73.4 ms, on the machine that produced this report (one thread). The target is under 2 s on a mid-range phone; that has not been measured.

## Distributions

### Radii of planets inside 100 days (R⊕)

```
    0.50 – 0.70       0.086  ###############################################
    0.70 – 1.00       0.063  ##################################
    1.00 – 1.25       0.079  ###########################################
    1.25 – 1.50       0.091  ##################################################
    1.50 – 1.75       0.089  #################################################
    1.75 – 2.00       0.044  ########################
    2.00 – 2.50       0.066  ####################################
    2.50 – 3.00       0.042  #######################
    3.00 – 4.00       0.004  ##
    4.00 – 6.00       0.000  
    6.00 – 10.00      0.039  #####################
   10.00 – 16.00      0.027  ##############
```

Counts are planets per star.

### Orbital periods of all planets (days)

```
    1.00 – 3.00       0.021  
    3.00 – 10.00      0.325  ####
   10.00 – 30.00      0.363  #####
   30.00 – 100.00     0.371  #####
  100.00 – 300.00     0.415  #####
  300.00 – 1000.00    0.634  ########
 1000.00 – 3000.00    0.580  ########
 3000.00 – 10000.00   0.843  ###########
10000.00 – 100000.00   3.525  ##################################################
100000.00 – 1000000.00   0.802  ###########
```

Counts are planets per star.

### Masses of all planets (M⊕)

```
    0.01 – 0.10       3.837  ##################################################
    0.10 – 0.30       1.597  ####################
    0.30 – 1.00       1.139  ##############
    1.00 – 3.00       0.721  #########
    3.00 – 10.00      0.373  ####
   10.00 – 30.00      0.030  
   30.00 – 100.00     0.040  
  100.00 – 300.00     0.081  #
  300.00 – 1000.00    0.055  
 1000.00 – 10000.00   0.008  
```

Counts are planets per star.
