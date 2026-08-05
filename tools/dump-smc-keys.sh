#!/bin/bash
# Enumerate every SMC key exposed by the patched applesmc on this MacPro7,1.
# Output: index<TAB>name<TAB>type<TAB>len<TAB>hexvalue
D=/sys/bus/acpi/devices/APP0001:00
n=$(cat "$D/key_count")
for ((i=0; i<n; i++)); do
  echo "$i" > "$D/key_at_index" 2>/dev/null || continue
  name=$(cat "$D/key_at_index_name" 2>/dev/null)
  type=$(cat "$D/key_at_index_type" 2>/dev/null)
  len=$(cat "$D/key_at_index_data_length" 2>/dev/null)
  hex=$(od -An -tx1 -v "$D/key_at_index_data" 2>/dev/null | tr -d ' \n')
  printf '%s\t%s\t%s\t%s\t%s\n' "$i" "$name" "$type" "$len" "$hex"
done
