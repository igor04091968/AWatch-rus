FROM python:3.11-slim

WORKDIR /app

RUN pip install prometheus_client requests

COPY collectors/aw_activitywatch.py /app/aw_activitywatch.py

CMD ["python3", "/app/aw_activitywatch.py"]
