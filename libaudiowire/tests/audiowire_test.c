#include "audiowire.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define CHANNELS 2
#define SAMPLE_RATE 48000
#define SAMPLE_FORMAT AW_SAMPLE_FORMAT_S16
#define PACKET_DURATION_MS 20
#define BUFFER_DURATION_MS 300
#define AUDIO_BUFSIZE 65536

#define assert_aw_result(res) assert(aw_result_is_ok(res))

int main() {
    char buf[AUDIO_BUFSIZE];
    aw_stream_t *record, *playback;
    aw_config_t config = {
        .channels = CHANNELS,
        .sample_rate = SAMPLE_RATE,
        .sample_format = SAMPLE_FORMAT,
        .buffer_duration = PACKET_DURATION_MS,
        .max_buffer_duration = BUFFER_DURATION_MS,
    };
    size_t bufsize = sizeof(buf);

    assert_aw_result(aw_initialize());
    assert_aw_result(aw_start_record(&record, NULL, config));
    assert_aw_result(aw_start_playback(&playback, NULL, config));

    assert(aw_device_name(record) != NULL);
    assert(aw_device_name(playback) != NULL);

    size_t read = 0;
    while (read <= 0) {
        read = aw_record_read(record, buf, bufsize);
        if (read > 0) {
            size_t write = aw_playback_write(playback, buf, read);
            assert(read == write);
        }
        usleep(20 * 1000);
    }

    assert_aw_result(aw_stop(playback));
    assert_aw_result(aw_stop(record));
    assert_aw_result(aw_terminate());

    return 0;
}
