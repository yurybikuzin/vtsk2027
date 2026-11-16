# =============================================================================
# Makefile – vtsk2027 research panel
# =============================================================================
#
# Цели:
#   all            полная сборка: cbr, rosstat, panel
#   force          то же, что all, но с FORCE=1 (перекачка всего)
#   build          собрать все крейты в tools/rs
#   cbr            скачать файлы ЦБ по реестру
#   rosstat        скачать файлы Росстата
#   panel          локальная сборка панели из data/given и data/downloaded
#   cbr19          извлечь инфляционные ожидания из CBR19
#   rD             извлечь ставку по депозитам из CBR05
#   k              ключевая ставка (SOAP)
#   y1             кривая бескупонной доходности, точка 1 год
#   moex           MCFTR и IMOEX
#   usd            курс доллара
#   quarterly-all  свернуть все длинные таблицы в кварталы
#   merge-all      приклеить все квартальные ряды к панели
#   enrich         производные столбцы
#   check          покрытие столбцов панели
#   export-csv     выгрузить панель в CSV для регрессий
#   regress-all    все регрессии H1–H5 и разности из (5)
#   tree           дерево проекта в tree.tools.txt и дерево data в tree.data.txt
#   bundle         собрать исходники, конфиги и ignore-файлы в bundle.txt
#   tree-clean     убрать сгенерированные tree-файлы
#   clean          убрать сгенерированные артефакты
#
# FORCE=1 перекачивает всё заново, включая k, y1, moex, usd.
# =============================================================================

SHELL := /bin/bash
.DEFAULT_GOAL := all

ROOT := $(CURDIR)

DATA_GIVEN       := $(ROOT)/data/given
DATA_DOWNLOADED  := $(ROOT)/data/downloaded/CBR_Deposits_Sources_Registry_2026-11-07
DATA_CONSTRUCTED := $(ROOT)/data/constructed
DATA_TMP         := $(ROOT)/data/tmp

RS       := $(ROOT)/tools/rs
BIN      := $(RS)/target/release
CARGO    := cargo
MANIFEST := $(RS)/Cargo.toml

CBR_PANEL := $(BIN)/cbr-panel
CBR_FETCH := $(BIN)/cbr-fetch
ROSSTAT_FETCH := $(BIN)/rosstat-fetch
COLLECT := $(BIN)/collect

REGISTRY := $(DATA_GIVEN)/CBR_Deposits_Sources_Registry_2026-11-07.csv

PANEL_XLSX     := $(DATA_CONSTRUCTED)/cbr_panel.xlsx
EXTRACT_LONG   := $(DATA_CONSTRUCTED)/extract_long.xlsx
K_LONG         := $(DATA_CONSTRUCTED)/k_long.xlsx
Y1_LONG        := $(DATA_CONSTRUCTED)/y1_long.xlsx
MOEX_LONG      := $(DATA_CONSTRUCTED)/moex_long.xlsx
USD_LONG       := $(DATA_CONSTRUCTED)/usd_long.xlsx
QUARTERLY      := $(DATA_CONSTRUCTED)/quarterly.xlsx
PANEL_ENRICHED := $(DATA_CONSTRUCTED)/cbr_panel_enriched.xlsx
PANEL_CSV      := $(DATA_CONSTRUCTED)/cbr_panel.csv

# алиасы под блок регрессий
CONSTRUCTED := $(DATA_CONSTRUCTED)
PANEL       := $(PANEL_ENRICHED)

REG_D_TOTAL   := $(CONSTRUCTED)/cbr_panel_reg_d_total.csv
REG_D_RUB     := $(CONSTRUCTED)/cbr_panel_reg_d_rub.csv
REG_D_TR      := $(CONSTRUCTED)/cbr_panel_reg_d_tr.csv
REG_SHIFT     := $(CONSTRUCTED)/cbr_panel_reg_shift.csv
REG_E         := $(CONSTRUCTED)/cbr_panel_reg_e.csv
REG_E2        := $(CONSTRUCTED)/cbr_panel_reg_e2.csv
REG_F         := $(CONSTRUCTED)/cbr_panel_reg_f.csv
REG_F2        := $(CONSTRUCTED)/cbr_panel_reg_f2.csv
REG_B         := $(CONSTRUCTED)/cbr_panel_reg_b.csv
REG_ED        := $(CONSTRUCTED)/cbr_panel_reg_ed.csv
REG_BE        := $(CONSTRUCTED)/cbr_panel_reg_be.csv
REG_FE        := $(CONSTRUCTED)/cbr_panel_reg_fe.csv
REGRESS_LAGS  := 3

MIN_Q := 2018Q1
MAX_Q := 2026Q2

ifeq ($(FORCE),1)
FORCE_FLAG := --force
else
FORCE_FLAG :=
endif

.PHONY: all force cbr rosstat panel cbr19 rD k y1 moex usd \
        quarterly-all merge-all enrich check export-csv \
        regress-all regress-h1-total regress-h1-rub regress-h4-tr regress-shift \
        regress-h2 regress-h2-ext regress-h3 regress-h3-ext regress-h5 \
        regress-ed regress-be regress-fe \
        tree bundle tree-clean clean build

all: cbr rosstat panel

force:
	$(MAKE) all FORCE=1

build:
	cd $(RS) && $(CARGO) build --release

cbr: $(CBR_FETCH)
	$(CBR_FETCH) --registry $(REGISTRY) --out-dir $(DATA_DOWNLOADED)

rosstat: $(ROSSTAT_FETCH)
	$(ROSSTAT_FETCH) links --out-dir $(DATA_DOWNLOADED)

panel: $(CBR_PANEL)
	$(CBR_PANEL) panel \
		--given $(DATA_GIVEN) \
		--downloaded $(DATA_DOWNLOADED) \
		--out $(PANEL_XLSX)

cbr19: $(CBR_PANEL)
	$(CBR_PANEL) extract-config \
		--config $(RS)/cbr-panel/config.toml \
		--root $(DATA_DOWNLOADED) \
		--out $(EXTRACT_LONG) \
		--only infl_exp_y1

rD: $(CBR_PANEL)
	$(CBR_PANEL) extract-config \
		--config $(RS)/cbr-panel/config.toml \
		--root $(DATA_DOWNLOADED) \
		--out $(EXTRACT_LONG) \
		--only rD

k: $(CBR_PANEL)
	@if [ -f "$(K_LONG)" ] && [ "$(FORCE)" != "1" ]; then \
		echo "skip k: $(K_LONG) exists"; \
	else \
		$(CBR_PANEL) fetch-key-rate --from 2018-01-01 --to 2026-12-31 --out $(K_LONG); \
	fi

y1: $(CBR_PANEL)
	@if [ -f "$(Y1_LONG)" ] && [ "$(FORCE)" != "1" ]; then \
		echo "skip y1: $(Y1_LONG) exists"; \
	else \
		$(CBR_PANEL) fetch-zcyc --from 2018-01-01 --to 2026-12-31 --out $(Y1_LONG); \
	fi

moex: $(CBR_PANEL)
	@if [ -f "$(MOEX_LONG)" ] && [ "$(FORCE)" != "1" ]; then \
		echo "skip moex: $(MOEX_LONG) exists"; \
	else \
		$(CBR_PANEL) fetch-moex --from 2017-01-01 --to 2026-12-31 --out $(MOEX_LONG); \
	fi

usd: $(CBR_PANEL)
	@if [ -f "$(USD_LONG)" ] && [ "$(FORCE)" != "1" ]; then \
		echo "skip usd: $(USD_LONG) exists"; \
	else \
		$(CBR_PANEL) fetch-usd --from 2018-01-01 --to 2026-12-31 --out $(USD_LONG); \
	fi

quarterly-all: k y1 moex usd
	$(CBR_PANEL) quarterly --in $(K_LONG) --out $(QUARTERLY) --append
	$(CBR_PANEL) quarterly --in $(Y1_LONG) --out $(QUARTERLY) --append
	$(CBR_PANEL) quarterly --in $(MOEX_LONG) --out $(QUARTERLY) --append
	$(CBR_PANEL) quarterly --in $(USD_LONG) --out $(QUARTERLY) --append

merge-all: quarterly-all
	$(CBR_PANEL) merge \
		--panel $(PANEL_XLSX) \
		--in $(QUARTERLY) \
		--out $(PANEL_ENRICHED) \
		--min-quarter $(MIN_Q) \
		--max-quarter $(MAX_Q)

enrich: merge-all
	$(CBR_PANEL) enrich --panel $(PANEL_ENRICHED) --out $(PANEL_ENRICHED)

check: enrich
	$(CBR_PANEL) check --panel $(PANEL_ENRICHED)

export-csv: enrich
	$(CBR_PANEL) export-csv --panel $(PANEL_ENRICHED) --out $(PANEL_CSV)

regress-all: regress-h1-total regress-h1-rub regress-h4-tr regress-shift \
             regress-h2 regress-h2-ext regress-h3 regress-h3-ext regress-h5 \
             regress-ed regress-be regress-fe

regress-h1-total:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_D_TOTAL) \
		--columns "f_D_oth_total,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_D_TOTAL) \
		--formula "f_D_oth_total ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h1-rub:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_D_RUB) \
		--columns "f_D_oth_rub,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_D_RUB) \
		--formula "f_D_oth_rub ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h4-tr:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_D_TR) \
		--columns "f_D_tr,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_D_TR) \
		--formula "f_D_tr ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-shift:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_SHIFT) \
		--columns "f_D_shift,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_SHIFT) \
		--formula "f_D_shift ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h2:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_E) \
		--columns "f_E,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_E) \
		--formula "f_E ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h2-ext:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_E2) \
		--columns "f_E,s_lag,y1_lag,q_DIV_lag,Vol_lag,m_lag,d_usd_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_E2) \
		--formula "f_E ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + m_lag + d_usd_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h3:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_F) \
		--columns "f_F,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_F) \
		--formula "f_F ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h3-ext:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_F2) \
		--columns "f_F,s_lag,y1_lag,q_DIV_lag,Vol_lag,m_lag,d_usd_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_F2) \
		--formula "f_F ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + m_lag + d_usd_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-h5:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_B) \
		--columns "f_B,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_B) \
		--formula "f_B ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-ed:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_ED) \
		--columns "g_E_D,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_ED) \
		--formula "g_E_D ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-be:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_BE) \
		--columns "g_B_E,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_BE) \
		--formula "g_B_E ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

regress-fe:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- export-csv \
		--panel $(PANEL) \
		--out $(REG_FE) \
		--columns "g_F_E,s_lag,y1_lag,q_DIV_lag,Vol_lag,Q2,Q3,Q4"
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- regress \
		--csv $(REG_FE) \
		--formula "g_F_E ~ s_lag + y1_lag + q_DIV_lag + Vol_lag + Q2 + Q3 + Q4" \
		--lags $(REGRESS_LAGS)

COLLECT := $(BIN)/collect

$(COLLECT): $(wildcard $(RS)/collect/src/*.rs) $(RS)/collect/Cargo.toml
	cd $(RS) && $(CARGO) build --release -p collect

tree: $(COLLECT)
	$(COLLECT) tree $(ROOT) > tree.tools.txt
	$(COLLECT) tree $(ROOT)/data > tree.data.txt

bundle: $(COLLECT)
	$(COLLECT) bundle $(ROOT) bundle.txt

tree-clean:
	rm -f tree.data.txt tree.tools.txt

clean: tree-clean
	rm -f bundle.txt
	rm -f $(PANEL_CSV)
