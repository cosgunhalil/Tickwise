/* The C test harness for tickwise-ffi.
 *
 * Records two sessions of a tiny deterministic simulation through the
 * shared library, one clean and one with a defect injected at a known
 * tick, then drives every misuse path and checks the status codes.
 *
 * Usage: harness <clean.rec> <chaotic.rec> <scratch-dir>
 *
 * Exit code 0 means every check passed. The Rust test in
 * tests/c_harness.rs compiles and runs this file, then reads the two
 * recordings back with the core reader, and CI runs `tickwise compare`
 * and `tickwise inspect` on them afterwards.
 */

#include "tickwise.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond)                                                         \
    do {                                                                    \
        if (!(cond)) {                                                      \
            failures++;                                                     \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);          \
        }                                                                   \
    } while (0)

#define CHECK_STATUS(expr, expected)                                        \
    do {                                                                    \
        enum TickwiseStatus got_ = (expr);                                  \
        if (got_ != (expected)) {                                           \
            failures++;                                                     \
            printf("FAIL %s:%d: %s returned %s, expected %s: %s\n",         \
                   __FILE__, __LINE__, #expr, tickwise_status_name(got_),   \
                   tickwise_status_name(expected),                          \
                   tickwise_last_error_message());                          \
        }                                                                   \
    } while (0)

/* xxh3_64 of the empty input. Proves the exported hash is real xxh3 and
 * not some other function that happens to return 64 bits. */
static const uint64_t XXH3_EMPTY = 0x2D06800538D394C2ULL;

#define TICKS 600u
#define DIVERGENCE_AT 421u
#define FULL_INTERVAL 50u
#define SNAPSHOT_EVERY 100u

/* A four-body integer simulation with its own LCG. No floats, no
 * platform behavior, so two runs agree bit for bit unless asked not to. */
typedef struct Sim {
    uint64_t pos[4];
    uint64_t score;
    uint32_t rng;
} Sim;

static uint32_t lcg_next(uint32_t *state) {
    *state = *state * 1664525u + 1013904223u;
    return *state;
}

static void sim_init(Sim *sim, uint32_t seed) {
    memset(sim, 0, sizeof *sim);
    sim->rng = seed;
}

static void sim_step(Sim *sim, const uint8_t inputs[2], int inject_defect) {
    size_t i;
    for (i = 0; i < 4; i++) {
        sim->pos[i] += (uint64_t)inputs[i % 2] + (lcg_next(&sim->rng) & 0xFu);
    }
    sim->score += sim->pos[0] % 7u;
    if (inject_defect) {
        /* A stale value leaking into gameplay state: the kind of bug
         * Tickwise exists to locate. */
        sim->score += 1;
    }
}

/* Serializes the state in a fixed little-endian layout so the hash never
 * sees struct padding. */
static size_t sim_encode(const Sim *sim, uint8_t out[44]) {
    size_t n = 0;
    size_t i;
    int b;
    for (i = 0; i < 4; i++) {
        for (b = 0; b < 8; b++) {
            out[n++] = (uint8_t)(sim->pos[i] >> (8 * b));
        }
    }
    for (b = 0; b < 8; b++) {
        out[n++] = (uint8_t)(sim->score >> (8 * b));
    }
    for (b = 0; b < 4; b++) {
        out[n++] = (uint8_t)(sim->rng >> (8 * b));
    }
    return n;
}

static uint64_t light_hash(const Sim *sim) {
    /* Score and the RNG only: cheap, and blind to positions on purpose so
     * the full hash has something to catch that the light hash misses. */
    uint8_t buf[12];
    int b;
    for (b = 0; b < 8; b++) {
        buf[b] = (uint8_t)(sim->score >> (8 * b));
    }
    for (b = 0; b < 4; b++) {
        buf[8 + b] = (uint8_t)(sim->rng >> (8 * b));
    }
    return tickwise_xxh3_64(buf, sizeof buf);
}

static uint64_t full_hash(const Sim *sim) {
    uint8_t buf[44];
    size_t n = sim_encode(sim, buf);
    return tickwise_xxh3_64(buf, n);
}

static int record_session(const char *path, int diverge) {
    static const char game_id[] = "c-harness";
    static const char platform[] = "ctest";
    static const char marker[] = "round start";
    struct TickwiseRecorderConfig config;
    struct TickwiseRecorder *rec = NULL;
    Sim sim;
    uint64_t tick;

    CHECK_STATUS(tickwise_recorder_config_default(&config), TICKWISE_STATUS_OK);
    config.game_id = (const uint8_t *)game_id;
    config.game_id_len = sizeof game_id - 1;
    config.platform = (const uint8_t *)platform;
    config.platform_len = sizeof platform - 1;
    config.tick_rate = 60;
    config.rng_seed = 12345;
    config.full_hash_interval = FULL_INTERVAL;
    config.snapshot_every = SNAPSHOT_EVERY;
    config.hash_algo_id = TICKWISE_HASH_ALGO_XXH3;
    config.input_format_id = 42;

    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)path, strlen(path), &config, &rec),
                 TICKWISE_STATUS_OK);
    if (rec == NULL) {
        return 1;
    }

    sim_init(&sim, (uint32_t)config.rng_seed);
    for (tick = 0; tick < TICKS; tick++) {
        uint8_t inputs[2];
        uint64_t light;
        uint64_t full = 0;
        inputs[0] = (uint8_t)(tick / 30u);
        inputs[1] = (uint8_t)((tick / 45u) & 1u);
        sim_step(&sim, inputs, diverge && tick >= DIVERGENCE_AT);

        light = light_hash(&sim);
        if (tickwise_recorder_wants_full_hash(rec, tick)) {
            full = full_hash(&sim);
        }
        CHECK_STATUS(tickwise_recorder_record_tick(rec, tick, inputs, sizeof inputs, light, full),
                     TICKWISE_STATUS_OK);

        if (tickwise_recorder_wants_snapshot(rec, tick)) {
            uint8_t snapshot[44];
            size_t n = sim_encode(&sim, snapshot);
            CHECK_STATUS(tickwise_recorder_record_snapshot(rec, tick, snapshot, n),
                         TICKWISE_STATUS_OK);
        }
        if (tick == 300) {
            CHECK_STATUS(tickwise_recorder_record_marker(rec, tick, (const uint8_t *)marker,
                                                         sizeof marker - 1),
                         TICKWISE_STATUS_OK);
        }
    }

    CHECK_STATUS(tickwise_recorder_finish(rec), TICKWISE_STATUS_OK);
    tickwise_recorder_destroy(rec);
    return 0;
}

static void check_versions_and_hash(void) {
    const char *version = tickwise_ffi_version();
    CHECK(tickwise_ffi_abi_version() == TICKWISE_ABI_VERSION);
    CHECK(version != NULL && strlen(version) > 0);
    CHECK(tickwise_xxh3_64(NULL, 0) == XXH3_EMPTY);
    CHECK(tickwise_xxh3_64((const uint8_t *)"", 0) == XXH3_EMPTY);
    CHECK(tickwise_xxh3_64(NULL, 8) == 0);
    CHECK(strlen(tickwise_status_name(TICKWISE_STATUS_IO)) > 0);
    CHECK(strlen(tickwise_status_name(TICKWISE_STATUS_PANIC)) > 0);
}

static void check_misuse_without_handle(void) {
    struct TickwiseRecorderConfig config;
    struct TickwiseRecorder *rec = NULL;
    static const uint8_t bad_utf8[] = {0xFF, 0xFE, '.', 'r', 'e', 'c'};
    static const char missing_dir[] = "definitely/not/a/dir/x.rec";

    CHECK_STATUS(tickwise_recorder_config_default(NULL), TICKWISE_STATUS_NULL_POINTER);
    CHECK(strlen(tickwise_last_error_message()) > 0);
    CHECK_STATUS(tickwise_recorder_config_default(&config), TICKWISE_STATUS_OK);
    CHECK(config.full_hash_interval == 300);
    CHECK(config.snapshot_every == 0);
    CHECK(config.game_id == NULL && config.game_id_len == 0);

    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)"x.rec", 5, &config, NULL),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)"x.rec", 5, NULL, &rec),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_create(NULL, 5, &config, &rec), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_create(NULL, 0, &config, &rec),
                 TICKWISE_STATUS_INVALID_ARGUMENT);
    CHECK_STATUS(tickwise_recorder_create(bad_utf8, sizeof bad_utf8, &config, &rec),
                 TICKWISE_STATUS_INVALID_UTF8);
    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)missing_dir, sizeof missing_dir - 1,
                                          &config, &rec),
                 TICKWISE_STATUS_IO);
    CHECK(rec == NULL);

    CHECK_STATUS(tickwise_recorder_record_tick(NULL, 0, NULL, 0, 0, 0),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK(!tickwise_recorder_wants_full_hash(NULL, 0));
    CHECK(!tickwise_recorder_wants_snapshot(NULL, 0));
    CHECK_STATUS(tickwise_recorder_record_snapshot(NULL, 0, NULL, 0),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_record_marker(NULL, 0, NULL, 0),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_finish(NULL), TICKWISE_STATUS_NULL_POINTER);
    tickwise_recorder_destroy(NULL);
}

static void check_misuse_with_handle(const char *scratch_dir) {
    char path[1024];
    struct TickwiseRecorderConfig config;
    struct TickwiseRecorder *rec = NULL;
    static uint8_t long_label[70000];
    static const uint8_t bad_utf8[] = {0xFF, 0xFE};
    int written;

    written = snprintf(path, sizeof path, "%s/misuse.rec", scratch_dir);
    CHECK(written > 0 && (size_t)written < sizeof path);
    memset(long_label, 'x', sizeof long_label);

    CHECK_STATUS(tickwise_recorder_config_default(&config), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)path, strlen(path), &config, &rec),
                 TICKWISE_STATUS_OK);
    if (rec == NULL) {
        return;
    }

    CHECK_STATUS(tickwise_recorder_record_tick(rec, 0, NULL, 4, 1, 0),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_record_tick(rec, 0, NULL, 0, 1, 0), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_recorder_record_tick(rec, 5, NULL, 0, 1, 0),
                 TICKWISE_STATUS_NON_SEQUENTIAL_TICK);
    CHECK(strstr(tickwise_last_error_message(), "expected 1, got 5") != NULL);
    CHECK_STATUS(tickwise_recorder_record_marker(rec, 0, long_label, sizeof long_label),
                 TICKWISE_STATUS_INVALID_ARGUMENT);
    CHECK_STATUS(tickwise_recorder_record_marker(rec, 0, bad_utf8, sizeof bad_utf8),
                 TICKWISE_STATUS_INVALID_UTF8);
    CHECK(tickwise_recorder_wants_full_hash(rec, 300));
    CHECK(!tickwise_recorder_wants_full_hash(rec, 301));
    CHECK(!tickwise_recorder_wants_snapshot(rec, 0));

    CHECK_STATUS(tickwise_recorder_finish(rec), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_recorder_finish(rec), TICKWISE_STATUS_ALREADY_FINISHED);
    CHECK_STATUS(tickwise_recorder_record_tick(rec, 1, NULL, 0, 1, 0),
                 TICKWISE_STATUS_ALREADY_FINISHED);
    CHECK(!tickwise_recorder_wants_full_hash(rec, 300));
    CHECK(!tickwise_recorder_wants_snapshot(rec, 0));
    tickwise_recorder_destroy(rec);
}

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "usage: %s <clean.rec> <chaotic.rec> <scratch-dir>\n", argv[0]);
        return 2;
    }

    check_versions_and_hash();
    check_misuse_without_handle();
    check_misuse_with_handle(argv[3]);
    if (record_session(argv[1], 0) != 0) {
        return 1;
    }
    if (record_session(argv[2], 1) != 0) {
        return 1;
    }

    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("harness ok: recorded %u ticks per session, defect injected at tick %u\n", TICKS,
           DIVERGENCE_AT);
    return 0;
}
