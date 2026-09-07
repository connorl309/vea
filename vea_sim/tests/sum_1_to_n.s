; sum_1_to_n.s - accumulate 1 + 2 + ... + 10 in a register loop.
; Known answer: 55. Exercises a forward-exit loop driven by an unsigned cmp
; (blt: N != V, and cmp leaves V = 0, so it is a plain "i < n").

        mov   r1, #0          ;= r1=0        ; acc
        mov   r2, #0          ;= r2=0        ; i
        mov   r3, #10         ;= r3=10       ; n

loop:   add   r2, r2, #1      ; i++
        add   r1, r1, r2      ; acc += i
        cmp   r2, r3
        blt   loop

        halt

;= at 5: r1=1 r2=1
;= final: r1=55 r2=10 r3=10
