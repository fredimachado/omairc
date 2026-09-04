FROM alpine:3.21
RUN apk add --no-cache ngircd
COPY configs/ngircd.conf /etc/ngircd/ngircd.conf
USER ngircd
EXPOSE 6667
CMD ["ngircd", "-n", "-f", "/etc/ngircd/ngircd.conf"]
