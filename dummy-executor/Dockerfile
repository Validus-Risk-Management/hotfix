FROM golang:1.25-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o /executor .

FROM alpine:3.21
COPY --from=build /executor /executor
COPY config/executor.cfg /config/executor.cfg
EXPOSE 9880
ENTRYPOINT ["/executor"]
