       01 ACCOUNT-RECORD.
          05 ACCOUNT-ID    PIC 9(6).
          05 STATUS-CODE   PIC X.
             88 ACTIVE      VALUE "A".
             88 CLOSED      VALUE "C".
             88 DELINQUENT  VALUE "D".
          05 BALANCE       PIC S9(7)V99 COMP-3.
          COPY ACCTNAME REPLACING ==:P:== BY ==CUST==.
          05 BRANCH-NO      PIC 9(4) COMP.
          05 RISK-SCORE     PIC 9(6) COMP-X.
          05 INTERNAL-ID    PIC S9(9) COMP-5.
          05 PRINT-BAL      PIC ZZ,ZZ9.99.
          05 ACCOUNT-SEQUENCE PIC 9(8) COMP-6.
