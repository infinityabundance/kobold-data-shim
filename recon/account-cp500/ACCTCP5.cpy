       01  ACCOUNT-RECORD.
           05  ACCOUNT-ID     PIC X(6).
           05  STATUS-CODE    PIC X.
               88  ACTIVE      VALUE "A".
               88  CLOSED      VALUE "C".
               88  DELINQUENT  VALUE "D".
           05  CUSTOMER-NAME  PIC X(12).
           05  CUST-TIER      PIC X.
               88  CUST-GOLD   VALUE "G".
           05  BALANCE        PIC S9(7)V99 COMP-3.
           05  BRANCH-NO      PIC 9(4) COMP.
           05  RISK-SCORE     PIC 9(6) COMP-X.
           05  INTERNAL-ID    PIC S9(9) COMP-5.
           05 REGION-CODE    PIC 9(3).
           05 LIMIT-AMT      PIC S9(7)V99.
           05 RISK-PERCENT   PIC 9(3)V99.
