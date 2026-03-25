#pragma once

#include <stdint.h>
#include <stdlib.h>

// Single initialization function for Rust to call
void init_shabal_all(void);

// Main mining function (the dispatcher)
void find_best_deadline_sph(char *scoops, uint64_t nonce_count, char *gensig,
                            uint64_t *best_deadline, uint64_t *best_offset);

// ALWAYS include all SIMD headers (we compile everything)
#include "shabal_base.h"      // Has init_shabal_base(), find_best_deadline_base()
#include "shabal_sse2.h"      // Has init_shabal_sse2(), find_best_deadline_sse2()
#include "shabal_avx.h"       // Has init_shabal_avx(), find_best_deadline_avx()
#include "shabal_avx2.h"      // Has init_shabal_avx2(), find_best_deadline_avx2()
#include "shabal_avx512f.h"   // Has init_shabal_avx512f(), find_best_deadline_avx512f()
#include "shabal_neon.h"      // Has init_shabal_neon(), find_best_deadline_neon()