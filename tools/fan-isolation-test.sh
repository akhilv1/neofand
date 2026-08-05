#!/bin/bash
# Verify per-fan independent control on MacPro7,1 applesmc.
# Ramps exactly one fan at a time and samples all four, to prove isolation.
set -u
D=/sys/bus/acpi/devices/APP0001:00
rpms() { for i in 1 2 3 4; do printf '%6s ' "$(cat $D/fan${i}_input)"; done; }

echo ">> stopping neofand"
systemctl stop neofand
sleep 3
echo "after stop (SMC in control): manual=$(for i in 1 2 3 4; do printf '%s' "$(cat $D/fan${i}_manual)"; done)  rpm=$(rpms)"

# Take manual control of all four, park at min.
for i in 1 2 3 4; do echo 1 > $D/fan${i}_manual; echo "$(cat $D/fan${i}_min)" > $D/fan${i}_output; done
sleep 6
echo "all parked at min:                          rpm=$(rpms)"
echo
printf '%-22s %6s %6s %6s %6s\n' "test" fan1 fan2 fan3 fan4

for t in 1 2 3 4; do
  hi=$(( $(cat $D/fan${t}_max) * 80 / 100 ))
  echo "$hi" > $D/fan${t}_output
  sleep 11
  printf 'fan%s -> %-14s %s\n' "$t" "${hi}rpm" "$(rpms)"
  echo "$(cat $D/fan${t}_min)" > $D/fan${t}_output
  sleep 8
done

echo
echo ">> restoring neofand"
for i in 1 2 3 4; do echo 0 > $D/fan${i}_manual; done
systemctl start neofand
sleep 4
echo "final: active=$(systemctl is-active neofand) manual=$(for i in 1 2 3 4; do printf '%s' "$(cat $D/fan${i}_manual)"; done) rpm=$(rpms)"
