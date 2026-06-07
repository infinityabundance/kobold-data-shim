       01 POLICY-RECORD.
          05 POLICY-NO     PIC X(10).
          05 RISK-CLASS    PIC 9.
             88 LOW-RISK    VALUE 1 THRU 3.
             88 HIGH-RISK   VALUE 7 THRU 9.
          05 PREMIUM       PIC S9(6)V99 COMP-3.
          05 TERM-MONTHS   PIC 9(3).
