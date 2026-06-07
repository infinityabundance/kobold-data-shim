       01 PAYROLL-RECORD.
          05 EMP-ID       PIC 9(5).
          05 DEPT         PIC X(4).
          05 PAY-TYPE     PIC X.
             88 SALARIED   VALUE "S".
             88 HOURLY     VALUE "H".
          05 GROSS-PAY    PIC S9(7)V99 COMP-3.
          05 DEDUCTIONS   PIC S9(5)V99 COMP-3.
          05 EMPLOYEE-NO    PIC 9(6) COMP.
          05 HOURS-BUCKET   PIC S9(4) COMP-5.
          05 DEDUCT-EDIT    PIC ZZ9.99CR.
