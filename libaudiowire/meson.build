project('libaudiowire', 'c', version: '0.2.0')

compiler = meson.get_compiler('c')

src = ['src/common.c']
inc = include_directories('include')
test_deps = []
deps = []

host_system = host_machine.system()
if host_system == 'linux'
    src += 'src/pulseaudio.c'
    deps += compiler.find_library('pulse', required: true)
elif host_system == 'darwin'
    src += 'src/coreaudio.c'
    # TODO: Figure out dependencies for CoreAudio
    deps += [
        dependency('CoreFoundation'),
        dependency('CoreAudio'),
        dependency('AudioToolbox'),
    ]
else
    src += 'src/portaudio.c'
    deps += compiler.find_library('portaudio', required: true)
endif

lib = library(
    'audiowire2',
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
)

install_headers('include/audiowire2.h')

test('audiowire test', audiowire_test)