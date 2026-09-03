movi r1, 10000 // we will sum `i` for i in 0..r1

sum_loop:
    addi r0, r0, 1 // inc our `i`
    add r3, r3, r0 // sum += i
    bne r0, r1, sum_loop

halt