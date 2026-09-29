# Cloud Run image for a single function. The build context is a staging directory holding
# only the function binary, built beforehand as a static x86_64 musl executable
# (just package), so the image adds nothing but CA certificates and a non-root user.
FROM gcr.io/distroless/static-debian12:nonroot

COPY function /function

ENTRYPOINT ["/function"]
