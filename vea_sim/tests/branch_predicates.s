; branch_predicates.s - drive every conditional branch (beq/bne/blt/bge/bgt/ble)
; down both its taken and not-taken edge, recording each correct outcome as a
; bit in r20. All six pairs correct => r20 = 0b111111 = 63. Any wrong branch
; jumps to `fail` and leaves r20 = -1.

        mov   r20, #0

        ; ---- EQ / NE : 5 == 5 ----
        mov   r1, #5
        cmp   r1, #5
        beq   eq_ok
        b     fail
eq_ok:  or    r20, r20, #1
        bne   fail              ; Z set, so must NOT branch
        or    r20, r20, #2

        ; ---- LT / GE : signed -3 < 4 ----
        mov   r2, -#3
        cmp.s r2, #4
        blt   lt_ok
        b     fail
lt_ok:  or    r20, r20, #4
        bge   fail              ; N != V, so must NOT branch
        or    r20, r20, #8

        ; ---- GT / LE : unsigned 10 > 2 ----
        mov   r3, #10
        cmp   r3, #2
        bgt   gt_ok
        b     fail
gt_ok:  or    r20, r20, #16
        ble   fail              ; !Z and N == V, so must NOT branch
        or    r20, r20, #32
        b     done

fail:   mov   r20, -#1
done:   halt

;= final: r20=63
