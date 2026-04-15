FROM python:3.13-slim
WORKDIR /app
COPY pyproject.toml requirements-dev.txt /app/
COPY src /app/src
COPY sql /app/sql
COPY ops /app/ops
COPY scripts /app/scripts
RUN pip install --no-cache-dir fastapi uvicorn pydantic jsonschema pyyaml
ENV PYTHONPATH=/app/src
ENV PRRO_GATEWAY_CONFIG=/app/ops/config.example.yaml
CMD ["python", "scripts/run_rest.py"]
