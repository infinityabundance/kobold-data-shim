       01 ACCOUNT-RECORD.
          05 ACCOUNT-ID    PIC 9(6).
          05 STATUS-CODE   PIC X.
             88 ACTIVE      VALUE "A".
             88 CLOSED      VALUE "C".
             88 DELINQUENT  VALUE "D".
          05 BALANCE       PIC S9(7)V99 COMP-3.
          COPY ACCTNAME REPLACING ==:P:== BY ==CUST==.
