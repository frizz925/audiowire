#include "audiowire.h"

size_t aw_sample_size(aw_sample_format_t format) {
    switch (format) {
    case AW_SAMPLE_FORMAT_S16:
        return sizeof(uint16_t);
    case AW_SAMPLE_FORMAT_F32:
        return sizeof(float);
    }
    return 0;
}