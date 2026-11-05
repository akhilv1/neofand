ARG CADDY_IMAGE=caddy:2-alpine
FROM ${CADDY_IMAGE} AS caddy
FROM neofand-lxc-base:ubuntu24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d && chmod 755 /usr/sbin/policy-rc.d && \
    apt-get update && apt-get install -y --no-install-recommends python3 ca-certificates && \
    apt-get clean && rm -rf /var/lib/apt/lists/* && \
    useradd --system --home-dir /var/lib/neofand --create-home --shell /usr/sbin/nologin neofand
COPY --from=caddy /usr/bin/caddy /usr/bin/caddy
COPY www /opt/neofand/www
COPY linux_api.py /opt/neofand/linux_api.py
COPY Caddyfile /etc/neofand/Caddyfile
COPY neofand-*.service /etc/systemd/system/
COPY README.md /opt/neofand/LXC-README.md
RUN mkdir -p /etc/neofand/access /var/lib/neofand/telemetry && chmod 755 /var/lib/neofand /var/lib/neofand/telemetry && \
    chown -R neofand:neofand /var/lib/neofand && \
    systemctl disable ssh.service ssh.socket postfix.service && \
    systemctl enable neofand-api.service neofand-web.service neofand-ssh-keys.service && \
    /usr/bin/caddy validate --config /etc/neofand/Caddyfile --adapter caddyfile && \
    rm -f /etc/ssh/ssh_host_* /var/lib/dbus/machine-id /root/.bash_history && \
    truncate -s 0 /etc/machine-id && ln -s /etc/machine-id /var/lib/dbus/machine-id && \
    passwd -l root && rm -f /usr/sbin/policy-rc.d && \
    find /var/log -type f -exec truncate -s 0 {} \;
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
