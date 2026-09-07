; factorial.s - 12! by repeated multiplication. Known answer: 479001600.
; The loop counts r2 down to zero; bgt after `cmp r2, #0` reduces to "r2 != 0"
; because r2 is never negative (unsigned compare, N stays 0).

        mov   r1, #1          ;= r1=1        ; running product
        mov   r2, #12          ;= r2=12        ; counter

fact:   mul   r1, r1, r2
        sub   r2, r2, #1
        cmp   r2, #0
        bgt   fact

        halt

;= final: r1=479001600 r2=0
