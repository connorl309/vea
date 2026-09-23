; branch_squash.s - a taken, unconditional branch must squash the instruction that
; Fetch already read from the fall-through path, before that instruction can write
; a register. The mul gives Fetch time to stage the branch and the poison mov
; ahead of Execute, so the branch and the squash land on the same cycle.
; r9 keeps its sentinel value only if the squash works.

        mov   r1, #3
        mov   r2, #4
        mov   r9, #1
        mul   r3, r1, r2
        b     skip
        mov   r9, -#1          ; must not retire: r9 would then read -1
skip:   halt

;= final: r9=1
