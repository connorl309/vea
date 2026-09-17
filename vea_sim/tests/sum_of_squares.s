mov r1, #0
mov r2, #0
mov r3, #10     ; 1 + 2^2 + 3^3 + ... = 385
mov r4, 0x4000
loop:
    add r1, r1, #1
    mul r5, r1, r1
    add r2, r2, r5
    st [r4], r2
    add r4, r4, #8
    cmp r1, r3
    blt loop

halt