#include <stddef.h>
#include <stdint.h>

#if defined(__aarch64__)
#include <arm_neon.h>

#if defined(__clang__)
__attribute__((target("arch=armv8.2-a+dotprod")))
#elif defined(__GNUC__)
#pragma GCC push_options
#pragma GCC target("arch=armv8.2-a+dotprod")
#endif
int32_t vectorkit_dot_i8_aarch64_dotprod(const int8_t *left, const int8_t *right, size_t length) {
    int32x4_t accumulator = vdupq_n_s32(0);
    size_t index = 0;

    for (; index + 16 <= length; index += 16) {
        int8x16_t left_values = vld1q_s8(left + index);
        int8x16_t right_values = vld1q_s8(right + index);
        accumulator = vdotq_s32(accumulator, left_values, right_values);
    }

    int32_t sum = vaddvq_s32(accumulator);
    for (; index < length; index += 1) {
        sum += (int32_t)left[index] * (int32_t)right[index];
    }

    return sum;
}
#if defined(__GNUC__) && !defined(__clang__)
#pragma GCC pop_options
#endif

#endif
