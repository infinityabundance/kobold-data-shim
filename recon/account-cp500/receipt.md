# Reconciliation receipt — account-cp500 (KOBOLD.DATA.3 + DATA.5)

EBCDIC **cp500** account family. **Alphanumeric** DISPLAY text decoded via `GNURUST.15`; **numeric**
DISPLAY (zoned sign) decoded via `GNURUST.17` (KOBOLD.DATA.5); **binary/packed** fields are RAW storage
and are NOT text-converted (passthrough, proven). Re-run is byte-stable.

| field | value |
|-------|-------|
| records | 120 |
| fields/record | 11 |
| unsupported | 0 |
| encoding | cp500 (gnucobol-3.2:ebcdic500_ascii8bit.ttbl), auto_detected=false |
| numeric_display | zoned_sign=GNURUST.17, code_page=cp500 |
| binary/packed passthrough | true |
| gnucobol-rs | 0.6.3 |
| decode_output_sha256 | `a8c539b6ff48ff013c87ac71d9d0bf3a20f13202ceb5e3edf6230f4f9f05eedf` |

Proven: cp500 text + zoned numeric decode; LEVEL-88 on decoded values; COMP/COMP-3/COMP-5/COMP-X
unchanged (`ebcdic_never_touches_binary_or_packed`); the cp500 zoned numeric fields legitimately differ
ASCII-vs-cp500 (encoding-sensitive); negatives fail closed.
