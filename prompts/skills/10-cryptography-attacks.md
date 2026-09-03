# 10. Cryptography Attacks

When: Analyzing cryptographic protocols, mathematical challenges, token signing, key exchanges, or custom ciphers.

## Mental model
Modern cryptographic algorithms are not broken by brute-forcing unbroken primitives; they break due to algebraic reductions, structural leakage, parameter misuse, and implementation flaws. High-tier CTF crypto is primarily Lattice Reduction (LLL), Polynomial Root Finding (Coppersmith), Elliptic Curve Singularities, and Oracle Feedback.

## Attack arc
- Identify Primitive & Algebraic Structure:
  - Classify primitive: RSA, Discrete Log (DSA/Diffie-Hellman), Elliptic Curves (ECDSA/Ed25519), Symmetric (AES/DES/ChaCha20), Lattice (NTRU/LWE/SIS), Hash/MAC, ZKP (zk-SNARK).
- Lattice-Based Cryptanalysis (SageMath / Python):
  - *LLL (Lenstra–Lenstra–Lovász) & BKZ Reduction:* Construct integer matrix lattices to solve Shortest Vector Problem (SVP) or Closest Vector Problem (CVP).
  - *Coppersmith’s Theorem & Small Roots:* Find small integer roots of univariate polynomials modulo $N$ ($x < N^{1/e}$) or bivariate polynomials (partial key recovery, shared primes, stereotyped messages).
  - *Hidden Number Problem (HNP) & ECDSA Nonce Bias:* If nonces $k$ leak even 2–4 bits (biased MSB/LSB), set up a matrix of linear relations and run LLL to recover the full private key $d$.
  - *Low-Density Knapsack / Merkle-Hellman:* Solve subset sum problems with density $< 0.9408$ via CLOS lattice reduction.
- RSA Attacks & Factorization:
  - *Small Exponent/Roots:* $e=3$ unpadded (cube root), Hastad Broadcast ($e$ identical plaintexts encrypted with different moduli $N_i$ via CRT).
  - *Small Private Exponent:* Wiener’s attack ($d < \frac{1}{3}N^{1/4}$ via continued fractions), Boneh-Durfee attack ($d < N^{0.292}$ via Coppersmith bivariate).
  - *Factorization Shortcuts:* $p \approx q$ (Fermat factorization), $p-1$ smooth (Pollard’s $p-1$), $p+1$ smooth (Williams $p+1$), Elliptic Curve Method (ECM), Common factor ($\gcd(N_1, N_2) > 1$).
  - *Related Message:* Franklin-Reiter attack ($\gcd(f_1(m), f_2(m)) \pmod N$).
- Elliptic Curve Cryptography (ECC) Exploitation:
  - *Smooth Order:* Pohlig-Hellman algorithm when curve order $\#E(\mathbb{F}_p)$ has only small prime factors.
  - *Anomalous Curves ($p = \#E(\mathbb{F}_p)$):* Smart’s attack (maps curve group to $p$-adic numbers in $\mathbb{Q}_p$, solving ECDLP in linear time).
  - *Invalid Curve Attack:* Provide points not lying on the curve if point validation is missing.
- Symmetric & PRNG Attacks:
  - *Padding Oracle:* Exploit differential error responses in CBC mode to decrypt/encrypt arbitrary blocks.
  - *Length Extension:* Append data to un-keyed hash functions (MD5, SHA-1, SHA-256) used in `H(secret || message)`.
  - *Mersenne Twister (MT19937):* Capture 624 32-bit outputs and untemper to clone internal PRNG state (`randcrack`).
- Zero-Knowledge Proofs (ZKP) / Circom:
  - Look for under-constrained signals where intermediate witness values are not enforced with `<==` (assignment only `=` without constraint `===`).

## Key techniques & primitives
- SageMath LLL Matrix Construction:
  `M = Matrix(ZZ, [...]); B = M.LLL(); # shortest vector reveals secret coefficients`.
- Coppersmith Small Roots:
  `P.<x> = PolynomialRing(Zmod(N)); f = (m_known + x)^e - c; roots = f.small_roots(X=2^bounds, beta=1.0)`.

## Tells & signals
- Partial nonce leaks in ECDSA signatures = construct HNP matrix and apply LLL.
- RSA public key with $e=3$ or $e=65537$ with known message prefixes (`flag{...}`) = Coppersmith stereotyped message.
- Curve order equal to field modulus $p$ = Smart's attack.
