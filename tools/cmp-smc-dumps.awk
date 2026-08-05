# Groups dumps by filename; prints keys stable within EVERY group but differing between groups.
FNR==1 { g = (FILENAME ~ /open/) ? "OPEN" : "CLOSED"; groups[g]=1 }
{ split($0,a,"\t"); name=a[2]; val=a[5]; k=name SUBSEP g
  if (!(k in seen)) { seen[k]=1; v[k]=val } else if (v[k]!=val) bad[k]=1
  keys[name]=1 }
END {
  for (k in keys) {
    ok=1; out=""; prev=""; diff=0; first=1
    for (g in groups) {
      kk=k SUBSEP g
      if (!(kk in seen) || (kk in bad)) { ok=0; break }
      if (!first && v[kk]!=prev) diff=1
      out = out sprintf("%s=%-32s ", g, v[kk]); prev=v[kk]; first=0
    }
    if (ok && diff) printf "%-6s %s\n", k, out
  }
}
