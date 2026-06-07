docs/roadmap/README.md

AWatch-rus Development Roadmap

Правила выполнения задач:

1. Все задачи выполняются последовательно.

2. Запрещено менять архитектуру без отдельного архитектурного задания.

3. Любое изменение должно проходить:
   
   - cargo fmt
   - cargo clippy
   - cargo test
   - cargo build --release
   - smoke tests

4. Каждый завершенный этап должен сопровождаться:
   
   - списком измененных файлов;
   - описанием изменений;
   - результатами проверок;
   - перечнем известных ограничений.

5. При конфликте между задачами:
   
   - более новая задача имеет приоритет;
   - архитектурные ограничения имеют приоритет над реализацией.

Текущий порядок выполнения:

TASK_002_PRODUCTION_HARDENING.md

TASK_003_EXPLAINABLE_KPI.md

TASK_004_RISK_NARRATIVE.md

TASK_005_RUST_AGENT_BASELINE.md

TASK_006_PFSENSE_CONTRACT_LAYER.md

TASK_007_CUSTOMER_DEMO_PACK.md

TASK_008_REGISTRY_READINESS.md

Основная архитектура:

Backend/API:
Rust

Agent:
Rust

Portal:
Server-side HTML + HTMX

Future Enterprise UI:
React + TypeScript

Future Desktop Forensics:
Tauri

Не использовать:
- ML
- LLM
- SaaS-зависимости без отдельного решения
- ложные заявления о реализованных интеграциях
