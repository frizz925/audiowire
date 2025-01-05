project('libaudiowire', 'c', version: '0.1.0')

compiler = meson.get_compiler('c')

src = ['src/ringbuf.c']
inc = include_directories('include')
test_deps = []
deps = []

host_system = host_machine.system()
if host_system == 'linux'
    src += 'src/pulseaudio.c'
    deps += compiler.find_library('pulse', required: true)
else
    src += 'src/portaudio.c'
    deps += compiler.find_library('portaudio', required: true)
endif
if host_system == 'windows'
    test_deps += compiler.find_library('ws2_32', required: true)
endif

lib = library(
    'audiowire',
    src,
    include_directories: inc,
    dependencies: deps,
    install: true,
)

audiowire_test = executable(
    'audiowire-test',
    'tests/audiowire_test.c',
    include_directories: inc,
    link_with: lib,
    dependencies: test_deps,
)
ringbuf_test = executable(
    'ringbuf-test',
    'tests/ringbuf_test.c',
    include_directories: inc,
    link_with: lib,
)

install_headers('include/audiowire.h')

test('ringbuf test', ringbuf_test)
test('audiowire test', audiowire_test)