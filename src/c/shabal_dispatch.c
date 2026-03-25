
#include "shabal.h"
#include <string.h>
#include <stdio.h>

#if defined(__x86_64__) || defined(_M_X64)

#ifdef _MSC_VER
#include <intrin.h>
#else
#include <cpuid.h>
#endif

#ifdef __GNUC__

static int detect_avx(void) {
#ifdef __APPLE__

    unsigned int eax, ebx, ecx, edx;
    

    __asm__ __volatile__ (
        "cpuid"
        : "=a" (eax), "=b" (ebx), "=c" (ecx), "=d" (edx)
        : "a" (1), "c" (0)
    );
    

    if (!(ecx & (1 << 27))) return 0;
    if (!(ecx & (1 << 28))) return 0;
    

    unsigned int xcr0_eax, xcr0_edx;
    __asm__ __volatile__ (
        "xgetbv"
        : "=a" (xcr0_eax), "=d" (xcr0_edx)
        : "c" (0)
    );
    return ((xcr0_eax & 0x6) == 0x6);
#else
    return __builtin_cpu_supports("avx");
#endif
}

static int detect_avx2(void) {
#ifdef __APPLE__

    if (!detect_avx()) return 0;
    
    unsigned int eax, ebx, ecx, edx;
    

    __asm__ __volatile__ (
        "cpuid"
        : "=a" (eax), "=b" (ebx), "=c" (ecx), "=d" (edx)
        : "a" (7), "c" (0)
    );
    
    return (ebx & (1 << 5)) != 0;
#else
    return __builtin_cpu_supports("avx2");
#endif
}

#ifdef __APPLE__
static int detect_avx512f(void) {

    return 0;
}
#else
static int detect_avx512f(void) {
    return __builtin_cpu_supports("avx512f");
}
#endif

static int detect_sse2(void) {

    return 1;
}

#else

static void safe_cpuid(unsigned int level, unsigned int* eax, unsigned int* ebx,
                       unsigned int* ecx, unsigned int* edx) {
#ifdef _MSC_VER
    int cpuInfo[4];
    __cpuid(cpuInfo, (int)level);
    *eax = cpuInfo[0];
    *ebx = cpuInfo[1];
    *ecx = cpuInfo[2];
    *edx = cpuInfo[3];
#else
    __asm__ __volatile__ (
        "cpuid"
        : "=a" (*eax), "=b" (*ebx), "=c" (*ecx), "=d" (*edx)
        : "a" (level), "c" (0)
    );
#endif
}

static void safe_cpuid_count(unsigned int level, unsigned int count,
                             unsigned int* eax, unsigned int* ebx,
                             unsigned int* ecx, unsigned int* edx) {
#ifdef _MSC_VER
    int cpuInfo[4];
    __cpuidex(cpuInfo, (int)level, (int)count);
    *eax = cpuInfo[0];
    *ebx = cpuInfo[1];
    *ecx = cpuInfo[2];
    *edx = cpuInfo[3];
#else
    __asm__ __volatile__ (
        "cpuid"
        : "=a" (*eax), "=b" (*ebx), "=c" (*ecx), "=d" (*edx)
        : "a" (level), "c" (count)
    );
#endif
}

static unsigned long long safe_xgetbv(unsigned int index) {
#ifdef _MSC_VER
    return _xgetbv(index);
#else
    unsigned int eax, edx;
    __asm__ __volatile__ (
        "xgetbv"
        : "=a" (eax), "=d" (edx)
        : "c" (index)
    );
    return ((unsigned long long)edx << 32) | eax;
#endif
}

static int detect_sse2(void) {

    return 1;
}

static int detect_avx(void) {
    unsigned int eax, ebx, ecx, edx;
    

    safe_cpuid(0, &eax, &ebx, &ecx, &edx);
    if (eax < 1) return 0;
    

    safe_cpuid(1, &eax, &ebx, &ecx, &edx);
    if (!(ecx & (1 << 27))) return 0;
    if (!(ecx & (1 << 28))) return 0;
    

    unsigned long long xcr0 = safe_xgetbv(0);
    return (xcr0 & 0x6) == 0x6;
}

static int detect_avx2(void) {

    if (!detect_avx()) return 0;
    
    unsigned int eax, ebx, ecx, edx;
    

    safe_cpuid(0, &eax, &ebx, &ecx, &edx);
    if (eax < 7) return 0;
    

    safe_cpuid_count(7, 0, &eax, &ebx, &ecx, &edx);
    return (ebx & (1 << 5)) != 0;
}

#ifdef __APPLE__
static int detect_avx512f(void) {

    return 0;
}
#else
static int detect_avx512f(void) {

    if (!detect_avx2()) return 0;
    
    unsigned int eax, ebx, ecx, edx;
    

    safe_cpuid(0, &eax, &ebx, &ecx, &edx);
    if (eax < 7) return 0;
    

    safe_cpuid_count(7, 0, &eax, &ebx, &ecx, &edx);
    if (!(ebx & (1 << 16))) return 0;
    

    unsigned long long xcr0 = safe_xgetbv(0);
    return (xcr0 & 0xE0) == 0xE0;
}
#endif

#endif
#endif

typedef void (*ShabalFunc)(char *, uint64_t, char *, uint64_t *, uint64_t *);

static ShabalFunc current_impl = NULL;
static int initialized = 0;

void init_shabal_all(void) {
    if (initialized) return;
    
    fprintf(stderr, "ANNE Miner CPU detection initializing\n");
    

    current_impl = find_best_deadline_base;
    
#if defined(__x86_64__) || defined(_M_X64)
    fprintf(stderr, "Your CPU Architecture: x86_64\n");
    

    int avx512_available = 0;
    int avx2_available = 0;
    int avx_available = 0;
    int sse2_available = 0;
    

#ifdef __APPLE__
    avx512_available = 0;
    fprintf(stderr, "SIMD AVX512F: DISABLED (macOS)\n");
#else
    avx512_available = detect_avx512f();
    fprintf(stderr, "SIMD AVX512F: %s\n", avx512_available ? "YES" : "NO");
#endif
    

    avx2_available = detect_avx2();
    fprintf(stderr, "SIMD AVX2: %s\n", avx2_available ? "YES" : "NO");
    

    avx_available = detect_avx();
    fprintf(stderr, "SIMD AVX: %s\n", avx_available ? "YES" : "NO");
    

    sse2_available = detect_sse2();
    fprintf(stderr, "SIMD SSE2: %s\n", sse2_available ? "YES" : "NO");
    

#ifndef __APPLE__
    if (avx512_available) {
        fprintf(stderr, "Selecting AVX512F implementation\n");
        current_impl = find_best_deadline_avx512f;
        init_shabal_avx512f();
        initialized = 1;
        return;
    }
#endif
    
    if (avx2_available) {
        fprintf(stderr, "Selecting AVX2 implementation\n");
        current_impl = find_best_deadline_avx2;
        init_shabal_avx2();
        initialized = 1;
        return;
    }
    
    if (avx_available) {
        fprintf(stderr, "Selecting AVX implementation\n");
        current_impl = find_best_deadline_avx;
        init_shabal_avx();
        initialized = 1;
        return;
    }
    
    if (sse2_available) {
        fprintf(stderr, "Selecting SSE2 implementation\n");
        current_impl = find_best_deadline_sse2;
        init_shabal_sse2();
        initialized = 1;
        return;
    }
    
    fprintf(stderr, "No SIMD features detected\n");
    
#elif defined(__aarch64__) || defined(__ARM_NEON)
    fprintf(stderr, "Architecture: ARM\n");
    

    fprintf(stderr, "Selecting NEON implementation\n");
    current_impl = find_best_deadline_neon;
    init_shabal_neon();
    initialized = 1;
    return;
    
#else
    fprintf(stderr, "Architecture: Unknown/Generic\n");
#endif
    

    fprintf(stderr, "Selecting base (non-SIMD) implementation\n");
    init_shabal_base();
    initialized = 1;
}

void find_best_deadline_sph(char *scoops, uint64_t nonce_count, char *gensig,
                           uint64_t *best_deadline, uint64_t *best_offset) {

    if (!initialized) {
        init_shabal_all();
    }
    

    current_impl(scoops, nonce_count, gensig, best_deadline, best_offset);
}