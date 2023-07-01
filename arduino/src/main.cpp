#include <Arduino.h>
#undef min
#undef max
#include <array>
#include <filters.hpp>
#include <memory>

using std::array;
using std::bit_cast;

constexpr auto BAUD_RATE = 115'200UL;
constexpr auto SAMPLING_FREQUENCY = uint32_t(1000);
constexpr auto END_TRANSMISSION_MARKER = uint32_t(0x7f'c0'00'00);
constexpr auto SYNC = bit_cast<uint32_t>(array{'S', 'Y', 'N', 'C'});

auto f = digital_filter<float>::create(num<0.29289322, 0.0, -0.29289322>,
                                       den<1.0, -0.58578644, 0.41421356>);

template <typename T> void transmit(T value) noexcept {
  Serial.write(reinterpret_cast<byte *>(&value), sizeof(T));
}

template <typename T> T receive() noexcept {
  T result;
  Serial.readBytes(reinterpret_cast<byte *>(&result), sizeof(T));
  return result;
}

void setup() {}

void loop() {
  Serial.begin(BAUD_RATE);
  while (receive<uint32_t>() != SYNC) {
    delay(150);
  }

  transmit(SAMPLING_FREQUENCY);
  Serial.flush();

  for (;;) {
    if (auto const sample = receive<float>();
        bit_cast<uint32_t>(sample) != END_TRANSMISSION_MARKER) {
      transmit(f.filter(sample));
    } else {
      f.reset();

      transmit(END_TRANSMISSION_MARKER);
      Serial.flush();
      Serial.end();

      break;
    }
  }
}
