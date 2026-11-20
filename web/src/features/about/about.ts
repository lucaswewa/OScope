// About (W4.3): the server's time up, in the units that say it shortly.

/** A time up, in seconds, in its two largest units: `3 d 4 h`, `2 h 5 min`, `4 min 12 s`. */
export function upFor(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  const days = Math.floor(whole / 86_400);
  const hours = Math.floor(whole / 3_600) % 24;
  const minutes = Math.floor(whole / 60) % 60;
  if (days > 0) return `${days} d ${hours} h`;
  if (hours > 0) return `${hours} h ${minutes} min`;
  if (minutes > 0) return `${minutes} min ${whole % 60} s`;
  return `${whole} s`;
}
