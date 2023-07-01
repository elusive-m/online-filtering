#include <concepts>

namespace detail {
consteval auto max(auto a, auto b) noexcept { return a > b ? a : b; }

template <auto I, typename T, auto N, auto C, auto... Cs>
constexpr T propagate(T (&state)[N]) noexcept {
  // Skip zero coefficients
  constexpr bool skip = T(C) == T(0);

  // Last element
  if constexpr (sizeof...(Cs) == 0) {
    if constexpr (skip) {
      return T(0);
    } else {
      return T(C) * state[I];
    }
  } else {
    if constexpr (skip) {
      return propagate<I + 1, T, N, Cs...>(state);
    } else {
      return T(C) * state[I] + propagate<I + 1, T, N, Cs...>(state);
    }
  }
}

template <typename T, auto N, auto I = (N - 1)>
constexpr void update(T value, T (&state)[N]) noexcept {
  if constexpr (I == 0) {
    state[I] = value;
  } else {
    state[I] = state[I - 1];
    update<T, N, I - 1>(value, state);
  }
}

template <typename T, auto N, auto I = 0>
constexpr void zero(T (&state)[N]) noexcept {
  if constexpr (I == N) {
    return;
  } else {
    state[I] = T(0);
    zero<T, N, I + 1>(state);
  }
}

template <typename T, auto... Cs> struct coefficients {};
template <auto G, auto... Cs> struct scaled_coefficients {
  static constexpr auto gain = G;
  static constexpr auto size = sizeof...(Cs);

  template <typename T, auto N>
  static constexpr T reduce(T (&state)[N]) noexcept {
    if constexpr (size > 0) {
      return propagate<0, T, N, Cs...>(state);
    } else {
      return T(0);
    }
  }
};

template <typename T, auto C, auto... Cs>
consteval auto scale(coefficients<T, C, Cs...>) noexcept {
  return scaled_coefficients<C, (Cs / C)...>{};
}

template <typename T, auto B, auto A> struct generic_filter {
  T state[max(B.size, A.size)]{};
  static constexpr auto gain = T(B.gain / A.gain);

  constexpr T filter(T x) &noexcept {
    const auto v = (x - A.reduce(state));
    const auto y = gain * (v + B.reduce(state));
    update(v, state);

    return y;
  }

  constexpr void reset() &noexcept { zero(state); }
  static consteval bool is_iir() noexcept { return (A.size > 0); }
  static consteval bool is_fir() noexcept { return !is_iir(); }
};

template <auto... Cs> using num = coefficients<struct numerator_tag, Cs...>;
template <auto... Cs> using den = coefficients<struct denominator_tag, Cs...>;
} // namespace detail

template <auto... Cs> inline constexpr auto num = detail::num<Cs...>{};
template <auto... Cs> inline constexpr auto den = detail::den<Cs...>{};

template <std::floating_point T> struct digital_filter {
  template <std::floating_point auto... Bs, std::floating_point auto... As>
  static consteval auto create(detail::num<Bs...> b,
                               detail::den<As...> a) noexcept {
    static_assert(sizeof...(Bs) > 0, "Empty numerator");
    static_assert(sizeof...(As) > 0, "Empty denominator");

    using namespace detail;
    return generic_filter<T, scale(b), scale(a)>{};
  }
};
