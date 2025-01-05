#include "audiowire.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define AUDIO_BUFSIZE 65536

#define assert_aw_result(res) assert(aw_result_is_ok(res))

int main() {
    char buf[AUDIO_BUFSIZE];
    aw_stream_t *record, *playback;
    size_t bufsize = sizeof(buf);

    assert_aw_result(aw_initialize());
    assert_aw_result(aw_start_record(&record, NULL));
    assert_aw_result(aw_start_playback(&playback, NULL));

    assert(aw_device_name(record) != NULL);
    assert(aw_device_name(playback) != NULL);

    sleep(1);
    size_t read = aw_record_read(record, buf, bufsize);
    assert(read > 0);
    size_t write = aw_playback_write(playback, buf, read);
    assert(read == write);

    assert_aw_result(aw_stop(playback));
    assert_aw_result(aw_stop(record));
    assert_aw_result(aw_terminate());

    return 0;
}