/* The C test harness for tickwise-ffi, both passes.
 *
 * Pass 1: records two sessions of a tiny deterministic simulation through
 * the shared library, one clean and one with a defect injected at a known
 * tick, with a state dump every 100 ticks built field by field.
 *
 * Pass 2: replays the clean recording with the same simulation, verifying
 * every hash and collecting dumps at two named ticks into a .dump file.
 *
 * Then every misuse path, checked for a status rather than a crash.
 *
 * Usage: harness <clean.rec> <chaotic.rec> <scratch-dir>
 *
 * Exit code 0 means every check passed. The Rust test in
 * tests/c_harness.rs compiles and runs this file, then reads the
 * recordings and the dump back with the core reader, and CI runs
 * `tickwise compare` and `tickwise inspect` on the recordings afterwards.
 */

#include "tickwise.h"

#include <stdio.h>
#include <string.h>

/* This harness is written against ABI 2. A header from another version
 * would compile against the wrong struct layout, so refuse it here. */
#if TICKWISE_ABI_VERSION != 2
#error "ctest/harness.c is written against TICKWISE_ABI_VERSION 2"
#endif

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

#define STR(s) ((const uint8_t *)(s)), (sizeof(s) - 1)

/* xxh3_64 of the empty input. Proves the exported hash is real xxh3 and
 * not some other function that happens to return 64 bits. */
static const uint64_t XXH3_EMPTY = 0x2D06800538D394C2ULL;

#define TICKS 600u
#define DIVERGENCE_AT 421u
#define FULL_INTERVAL 50u
#define SNAPSHOT_EVERY 100u
#define DUMP_INTERVAL 100u

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
        sim->score += 1;
    }
}

static void sim_inputs(uint64_t tick, uint8_t out[2]) {
    out[0] = (uint8_t)(tick / 30u);
    out[1] = (uint8_t)((tick / 45u) & 1u);
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

/* The dump builder, the way an engine fills it: one call per field, with
 * a length for the collection so the diff can tell a shorter list from
 * a matching tail. */
static void sim_dump(const Sim *sim, uint64_t tick, struct TickwiseDump *dump) {
    char path[32];
    size_t i;
    CHECK_STATUS(tickwise_dump_clear(dump), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_dump_set_u64(dump, STR("tick"), tick), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_dump_set_u64(dump, STR("score"), sim->score), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_dump_set_u64(dump, STR("rng"), sim->rng), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_dump_set_len(dump, STR("pos"), 4), TICKWISE_STATUS_OK);
    for (i = 0; i < 4; i++) {
        int n = snprintf(path, sizeof path, "pos[%u]", (unsigned)i);
        CHECK_STATUS(tickwise_dump_set_u64(dump, (const uint8_t *)path, (size_t)n, sim->pos[i]),
                     TICKWISE_STATUS_OK);
    }
    CHECK(tickwise_dump_len(dump) == 8);
}

static int record_session(const char *path, int diverge) {
    static const char game_id[] = "c-harness";
    static const char platform[] = "ctest";
    static const char marker[] = "round start";
    struct TickwiseRecorderConfig config;
    struct TickwiseRecorder *rec = NULL;
    struct TickwiseDump *dump = tickwise_dump_new();
    Sim sim;
    uint64_t tick;

    CHECK_STATUS(tickwise_recorder_config_default(&config), TICKWISE_STATUS_OK);
    CHECK(config.dump_interval == 0);
    config.game_id = (const uint8_t *)game_id;
    config.game_id_len = sizeof game_id - 1;
    config.platform = (const uint8_t *)platform;
    config.platform_len = sizeof platform - 1;
    config.tick_rate = 60;
    config.rng_seed = 12345;
    config.full_hash_interval = FULL_INTERVAL;
    config.snapshot_every = SNAPSHOT_EVERY;
    config.dump_interval = DUMP_INTERVAL;
    config.hash_algo_id = TICKWISE_HASH_ALGO_XXH3;
    config.input_format_id = 42;

    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)path, strlen(path), &config, &rec),
                 TICKWISE_STATUS_OK);
    if (rec == NULL) {
        tickwise_dump_destroy(dump);
        return 1;
    }

    sim_init(&sim, (uint32_t)config.rng_seed);
    for (tick = 0; tick < TICKS; tick++) {
        uint8_t inputs[2];
        uint64_t light;
        uint64_t full = 0;
        sim_inputs(tick, inputs);
        sim_step(&sim, inputs, diverge && tick >= DIVERGENCE_AT);

        light = light_hash(&sim);
        if (tickwise_recorder_wants_full_hash(rec, tick)) {
            full = full_hash(&sim);
        }
        CHECK_STATUS(tickwise_recorder_record_tick(rec, tick, inputs, sizeof inputs, light, full),
                     TICKWISE_STATUS_OK);

        CHECK(tickwise_recorder_wants_dump(rec, tick) == (tick % DUMP_INTERVAL == 0));
        if (tickwise_recorder_wants_dump(rec, tick)) {
            sim_dump(&sim, tick, dump);
            CHECK_STATUS(tickwise_recorder_record_dump(rec, tick, dump), TICKWISE_STATUS_OK);
        }
        if (tickwise_recorder_wants_snapshot(rec, tick)) {
            uint8_t snapshot[44];
            size_t n = sim_encode(&sim, snapshot);
            CHECK_STATUS(tickwise_recorder_record_snapshot(rec, tick, snapshot, n),
                         TICKWISE_STATUS_OK);
        }
        if (tick == 300) {
            CHECK_STATUS(tickwise_recorder_record_marker(rec, tick, STR(marker)),
                         TICKWISE_STATUS_OK);
        }
    }

    CHECK_STATUS(tickwise_recorder_finish(rec), TICKWISE_STATUS_OK);
    tickwise_recorder_destroy(rec);
    tickwise_dump_destroy(dump);
    return 0;
}

/* Pass 2: the clean recording replayed by the same simulation. Every hash
 * must match, and dumps are collected at ticks 100 and 421. */
static int replay_session(const char *rec_path, const char *dump_path) {
    static const uint64_t dump_at[2] = {100, DIVERGENCE_AT};
    struct TickwiseReplayer *rep = NULL;
    struct TickwiseDump *dump = tickwise_dump_new();
    Sim sim;
    uint64_t first = 99, last = 99, tick;
    const uint8_t *inputs;
    size_t inputs_len;
    uint64_t steps = 0;
    int ok = 1;

    CHECK_STATUS(tickwise_replayer_open((const uint8_t *)rec_path, strlen(rec_path), dump_at, 2,
                                        true, true, 42, &rep),
                 TICKWISE_STATUS_OK);
    if (rep == NULL) {
        tickwise_dump_destroy(dump);
        return 1;
    }
    CHECK_STATUS(tickwise_replayer_tick_range(rep, &first, &last), TICKWISE_STATUS_OK);
    CHECK(first == 0 && last == TICKS - 1);

    sim_init(&sim, 12345);
    while (tickwise_replayer_next_step(rep, &tick, &inputs, &inputs_len)) {
        uint8_t expected[2];
        uint64_t full = 0;
        const struct TickwiseDump *dump_arg = NULL;
        enum TickwiseStatus status;

        sim_inputs(tick, expected);
        CHECK(inputs_len == 2 && memcmp(inputs, expected, 2) == 0);
        sim_step(&sim, inputs, 0);

        if (tickwise_replayer_wants_full_hash(rep, tick)) {
            full = full_hash(&sim);
        }
        if (tickwise_replayer_wants_dump(rep, tick)) {
            sim_dump(&sim, tick, dump);
            dump_arg = dump;
        }
        status = tickwise_replayer_after_tick(rep, light_hash(&sim), full, dump_arg);
        if (status != TICKWISE_STATUS_OK) {
            printf("FAIL replay at tick %llu: %s\n", (unsigned long long)tick,
                   tickwise_last_error_message());
            failures++;
            ok = 0;
            break;
        }
        steps++;
    }
    CHECK(steps == TICKS);
    CHECK_STATUS(tickwise_replayer_finish(rep, (const uint8_t *)dump_path, strlen(dump_path)),
                 TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_replayer_finish(rep, (const uint8_t *)dump_path, strlen(dump_path)),
                 TICKWISE_STATUS_ALREADY_FINISHED);
    tickwise_replayer_destroy(rep);
    tickwise_dump_destroy(dump);
    return ok ? 0 : 1;
}

static void check_versions_and_hash(void) {
    const char *version = tickwise_ffi_version();
    CHECK(tickwise_ffi_abi_version() == TICKWISE_ABI_VERSION);
    CHECK(version != NULL && strlen(version) > 0);
    CHECK(tickwise_xxh3_64(NULL, 0) == XXH3_EMPTY);
    CHECK(tickwise_xxh3_64((const uint8_t *)"", 0) == XXH3_EMPTY);
    CHECK(tickwise_xxh3_64(NULL, 8) == 0);
    CHECK(strlen(tickwise_status_name(TICKWISE_STATUS_IO)) > 0);
    CHECK(strlen(tickwise_status_name(TICKWISE_STATUS_MISSING_DUMP)) > 0);
}

static void check_misuse_without_handle(void) {
    struct TickwiseRecorderConfig config;
    struct TickwiseRecorder *rec = NULL;
    struct TickwiseReplayer *rep = NULL;
    static const uint8_t bad_utf8[] = {0xFF, 0xFE, '.', 'r', 'e', 'c'};
    static const char missing_dir[] = "definitely/not/a/dir/x.rec";
    uint64_t tick;
    const uint8_t *inputs;
    size_t len;

    CHECK_STATUS(tickwise_recorder_config_default(NULL), TICKWISE_STATUS_NULL_POINTER);
    CHECK(strlen(tickwise_last_error_message()) > 0);
    CHECK_STATUS(tickwise_recorder_config_default(&config), TICKWISE_STATUS_OK);
    CHECK(config.full_hash_interval == 300);
    CHECK(config.snapshot_every == 0);
    CHECK(config.game_id == NULL && config.game_id_len == 0);

    CHECK_STATUS(tickwise_recorder_create(STR("x.rec"), &config, NULL), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_create(STR("x.rec"), NULL, &rec), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_create(NULL, 5, &config, &rec), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_create(NULL, 0, &config, &rec), TICKWISE_STATUS_INVALID_ARGUMENT);
    CHECK_STATUS(tickwise_recorder_create(bad_utf8, sizeof bad_utf8, &config, &rec),
                 TICKWISE_STATUS_INVALID_UTF8);
    CHECK_STATUS(tickwise_recorder_create(STR(missing_dir), &config, &rec), TICKWISE_STATUS_IO);
    CHECK(rec == NULL);

    CHECK_STATUS(tickwise_recorder_record_tick(NULL, 0, NULL, 0, 0, 0), TICKWISE_STATUS_NULL_POINTER);
    CHECK(!tickwise_recorder_wants_full_hash(NULL, 0));
    CHECK(!tickwise_recorder_wants_snapshot(NULL, 0));
    CHECK(!tickwise_recorder_wants_dump(NULL, 0));
    CHECK_STATUS(tickwise_recorder_record_dump(NULL, 0, NULL), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_record_snapshot(NULL, 0, NULL, 0), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_record_marker(NULL, 0, NULL, 0), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_recorder_finish(NULL), TICKWISE_STATUS_NULL_POINTER);
    tickwise_recorder_destroy(NULL);

    CHECK_STATUS(tickwise_replayer_open(STR(missing_dir), NULL, 0, true, false, 0, &rep),
                 TICKWISE_STATUS_IO);
    CHECK(rep == NULL);
    CHECK(!tickwise_replayer_next_step(NULL, &tick, &inputs, &len));
    CHECK(!tickwise_replayer_wants_dump(NULL, 0));
    CHECK_STATUS(tickwise_replayer_after_tick(NULL, 0, 0, NULL), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_replayer_seek_to(NULL, 0), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_replayer_finish(NULL, STR("x.dump")), TICKWISE_STATUS_NULL_POINTER);
    tickwise_replayer_destroy(NULL);

    CHECK(tickwise_dump_len(NULL) == 0);
    CHECK_STATUS(tickwise_dump_clear(NULL), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_dump_set_u64(NULL, STR("x"), 1), TICKWISE_STATUS_NULL_POINTER);
    tickwise_dump_destroy(NULL);
}

static void check_misuse_with_handles(const char *scratch_dir, const char *clean_rec) {
    char path[1024];
    struct TickwiseRecorderConfig config;
    struct TickwiseRecorder *rec = NULL;
    struct TickwiseReplayer *rep = NULL;
    struct TickwiseDump *dump = tickwise_dump_new();
    static uint8_t long_label[70000];
    static const uint8_t bad_utf8[] = {0xFF, 0xFE};
    static const uint64_t far_tick[1] = {TICKS + 5};
    static const uint64_t dump_at_two[1] = {2};
    uint64_t tick;
    const uint8_t *inputs;
    size_t len;
    int written;

    written = snprintf(path, sizeof path, "%s/misuse.rec", scratch_dir);
    CHECK(written > 0 && (size_t)written < sizeof path);
    memset(long_label, 'x', sizeof long_label);

    /* The dump builder rejects what it cannot store, and keeps working. */
    CHECK_STATUS(tickwise_dump_set_u64(dump, NULL, 0, 1), TICKWISE_STATUS_INVALID_ARGUMENT);
    CHECK_STATUS(tickwise_dump_set_u64(dump, NULL, 3, 1), TICKWISE_STATUS_NULL_POINTER);
    CHECK_STATUS(tickwise_dump_set_str(dump, STR("s"), bad_utf8, sizeof bad_utf8),
                 TICKWISE_STATUS_INVALID_UTF8);
    CHECK_STATUS(tickwise_dump_set_u64(dump, STR("x"), 1), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_dump_set_u64(dump, STR("x"), 2), TICKWISE_STATUS_OK);
    CHECK(tickwise_dump_len(dump) == 1);

    /* The recorder. */
    CHECK_STATUS(tickwise_recorder_config_default(&config), TICKWISE_STATUS_OK);
    CHECK_STATUS(tickwise_recorder_create((const uint8_t *)path, strlen(path), &config, &rec),
                 TICKWISE_STATUS_OK);
    if (rec != NULL) {
        CHECK_STATUS(tickwise_recorder_record_tick(rec, 0, NULL, 4, 1, 0), TICKWISE_STATUS_NULL_POINTER);
        CHECK_STATUS(tickwise_recorder_record_tick(rec, 0, NULL, 0, 1, 0), TICKWISE_STATUS_OK);
        CHECK_STATUS(tickwise_recorder_record_tick(rec, 5, NULL, 0, 1, 0),
                     TICKWISE_STATUS_NON_SEQUENTIAL_TICK);
        CHECK(strstr(tickwise_last_error_message(), "expected 1, got 5") != NULL);
        CHECK_STATUS(tickwise_recorder_record_marker(rec, 0, long_label, sizeof long_label),
                     TICKWISE_STATUS_INVALID_ARGUMENT);
        CHECK_STATUS(tickwise_recorder_record_marker(rec, 0, bad_utf8, sizeof bad_utf8),
                     TICKWISE_STATUS_INVALID_UTF8);
        CHECK_STATUS(tickwise_recorder_record_dump(rec, 0, NULL), TICKWISE_STATUS_NULL_POINTER);
        CHECK(!tickwise_recorder_wants_dump(rec, 0));
        CHECK_STATUS(tickwise_recorder_record_dump(rec, 0, dump), TICKWISE_STATUS_OK);
        CHECK(tickwise_recorder_wants_full_hash(rec, 300));
        CHECK(!tickwise_recorder_wants_full_hash(rec, 301));

        CHECK_STATUS(tickwise_recorder_finish(rec), TICKWISE_STATUS_OK);
        CHECK_STATUS(tickwise_recorder_finish(rec), TICKWISE_STATUS_ALREADY_FINISHED);
        CHECK_STATUS(tickwise_recorder_record_tick(rec, 1, NULL, 0, 1, 0),
                     TICKWISE_STATUS_ALREADY_FINISHED);
        CHECK(!tickwise_recorder_wants_dump(rec, 0));
        tickwise_recorder_destroy(rec);
    }

    /* The replayer. */
    CHECK_STATUS(tickwise_replayer_open((const uint8_t *)clean_rec, strlen(clean_rec), NULL, 0,
                                        true, true, 41, &rep),
                 TICKWISE_STATUS_INPUT_FORMAT_MISMATCH);
    CHECK_STATUS(tickwise_replayer_open((const uint8_t *)clean_rec, strlen(clean_rec), far_tick, 1,
                                        true, false, 0, &rep),
                 TICKWISE_STATUS_TICK_OUT_OF_RANGE);
    CHECK_STATUS(tickwise_replayer_open((const uint8_t *)clean_rec, strlen(clean_rec), NULL, 3,
                                        true, false, 0, &rep),
                 TICKWISE_STATUS_NULL_POINTER);
    CHECK(rep == NULL);

    CHECK_STATUS(tickwise_replayer_open((const uint8_t *)clean_rec, strlen(clean_rec), dump_at_two,
                                        1, true, false, 0, &rep),
                 TICKWISE_STATUS_OK);
    if (rep != NULL) {
        Sim sim;
        sim_init(&sim, 12345);
        /* after_tick before any step. */
        CHECK_STATUS(tickwise_replayer_after_tick(rep, 0, 0, NULL), TICKWISE_STATUS_PROTOCOL_MISUSE);
        /* Two honest steps. */
        CHECK(tickwise_replayer_next_step(rep, &tick, &inputs, &len));
        sim_step(&sim, inputs, 0);
        CHECK_STATUS(tickwise_replayer_after_tick(rep, light_hash(&sim), full_hash(&sim), NULL),
                     TICKWISE_STATUS_OK);
        CHECK(tickwise_replayer_next_step(rep, &tick, &inputs, &len));
        sim_step(&sim, inputs, 0);
        CHECK_STATUS(tickwise_replayer_after_tick(rep, light_hash(&sim), 0, NULL), TICKWISE_STATUS_OK);
        /* Tick 2 owes a dump. */
        CHECK(tickwise_replayer_next_step(rep, &tick, &inputs, &len));
        CHECK(tick == 2 && tickwise_replayer_wants_dump(rep, 2));
        sim_step(&sim, inputs, 0);
        CHECK_STATUS(tickwise_replayer_after_tick(rep, light_hash(&sim), 0, NULL),
                     TICKWISE_STATUS_MISSING_DUMP);
        CHECK_STATUS(tickwise_replayer_after_tick(rep, light_hash(&sim), 0, dump), TICKWISE_STATUS_OK);
        /* A wrong hash is a mismatch at its tick, not a crash. */
        CHECK(tickwise_replayer_next_step(rep, &tick, &inputs, &len));
        sim_step(&sim, inputs, 0);
        CHECK_STATUS(tickwise_replayer_after_tick(rep, light_hash(&sim) ^ 1, 0, NULL),
                     TICKWISE_STATUS_HASH_MISMATCH);
        CHECK(strstr(tickwise_last_error_message(), "tick 3") != NULL);
        /* A skipped step is caught at finish, and the session survives. */
        CHECK(tickwise_replayer_next_step(rep, &tick, &inputs, &len));
        sim_step(&sim, inputs, 0);
        CHECK(tickwise_replayer_next_step(rep, &tick, &inputs, &len));
        sim_step(&sim, inputs, 0);
        CHECK(tick == 5);
        CHECK_STATUS(tickwise_replayer_after_tick(rep, light_hash(&sim), 0, NULL), TICKWISE_STATUS_OK);
        written = snprintf(path, sizeof path, "%s/misuse.dump", scratch_dir);
        CHECK(written > 0 && (size_t)written < sizeof path);
        CHECK_STATUS(tickwise_replayer_finish(rep, (const uint8_t *)path, strlen(path)),
                     TICKWISE_STATUS_PROTOCOL_MISUSE);
        CHECK_STATUS(tickwise_replayer_seek_to(rep, TICKS + 10), TICKWISE_STATUS_TICK_OUT_OF_RANGE);
        tickwise_replayer_destroy(rep);
    }
    tickwise_dump_destroy(dump);
}

int main(int argc, char **argv) {
    char dump_path[1024];
    int written;

    if (argc != 4) {
        fprintf(stderr, "usage: %s <clean.rec> <chaotic.rec> <scratch-dir>\n", argv[0]);
        return 2;
    }
    written = snprintf(dump_path, sizeof dump_path, "%s/clean.dump", argv[3]);
    if (written <= 0 || (size_t)written >= sizeof dump_path) {
        return 2;
    }

    check_versions_and_hash();
    check_misuse_without_handle();
    if (record_session(argv[1], 0) != 0) {
        return 1;
    }
    if (record_session(argv[2], 1) != 0) {
        return 1;
    }
    if (replay_session(argv[1], dump_path) != 0) {
        return 1;
    }
    check_misuse_with_handles(argv[3], argv[1]);

    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("harness ok: recorded %u ticks per session, defect injected at tick %u, "
           "replayed the clean session into %s\n",
           TICKS, DIVERGENCE_AT, dump_path);
    return 0;
}
