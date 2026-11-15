# =============================================================================
# vtsk2027 – сборка панели по депозитам и акциям
# =============================================================================

SHELL := /bin/bash
.DEFAULT_GOAL := help

DATA          := data
DOWNLOADED    := $(DATA)/downloaded/CBR_Deposits_Sources_Registry_2026-11-07
CONSTRUCTED   := $(DATA)/constructed
TMP           := $(DATA)/tmp

REGISTRY      := $(DATA)/given/CBR_Deposits_Sources_Registry_2026-11-07.csv

PANEL         := $(CONSTRUCTED)/cbr_panel.xlsx
CBR19_LONG    := $(CONSTRUCTED)/cbr19_infl_long.xlsx
CBR19_Q       := $(CONSTRUCTED)/cbr19_infl_quarterly.xlsx
RD_LONG       := $(CONSTRUCTED)/rD_long.xlsx
RD_Q          := $(CONSTRUCTED)/rD_quarterly.xlsx
K_LONG        := $(CONSTRUCTED)/k_long.xlsx
K_Q           := $(CONSTRUCTED)/k_quarterly.xlsx
Y1_LONG       := $(CONSTRUCTED)/y1_long.xlsx
Y1_Q          := $(CONSTRUCTED)/y1_quarterly.xlsx
MOEX_LONG     := $(CONSTRUCTED)/moex_long.xlsx
MOEX_Q        := $(CONSTRUCTED)/moex_quarterly.xlsx
USD_LONG      := $(CONSTRUCTED)/usd_long.xlsx
USD_Q         := $(CONSTRUCTED)/usd_quarterly.xlsx

CONFIG        := tools/rs/cbr-panel/config.toml
CARGO         := cargo
MANIFEST      := tools/rs/Cargo.toml

TREE_DATA     := tree.data.txt
TREE_TOOLS    := tree.tools.txt
RS_BUNDLE     := rs-bundle.txt
TREE_SH       := tools/sh/tree.sh
BUNDLE_SH     := tools/sh/collect-rs.sh

MIN_Q         := 2018Q1
MAX_Q         := 2026Q2

FORCE         ?=

.PHONY: help all force cbr rosstat tree bundle-tools tree-clean \
        panel cbr19 rD k y1 moex usd \
        quarterly-all merge-all enrich check \
        clean

help:
	@echo "Цели:"
	@echo "  all            всё, кроме Росстата: скачать ЦБ, собрать панель, обогатить, проверить"
	@echo "  force          то же, что all, но с FORCE=1, все fetch перекачиваются"
	@echo "  cbr            скачать файлы ЦБ по реестру"
	@echo "  rosstat        скачать Росстат (может упасть на 404, отдельно от all)"
	@echo "  panel          собрать базовую панель из CBR01..CBR04"
	@echo "  cbr19          инфляционные ожидания из CBR19"
	@echo "  rD             ставка по депозитам из CBR05"
	@echo "  k              ключевая ставка через SOAP"
	@echo "  y1             кривая ОФЗ, точка 1 год"
	@echo "  moex           MCFTR/IMOEX: Vol, R_E, q_DIV"
	@echo "  usd            курс доллара через XML_dynamic"
	@echo "  quarterly-all  свернуть все Long в кварталы"
	@echo "  merge-all      приклеить все кварталы к панели"
	@echo "  enrich         добавить s, m, d_usd, outlier"
	@echo "  check          отчёт по покрытию"
	@echo "  tree           обновить tree.data.txt, tree.tools.txt, rs-bundle.txt"
	@echo "  bundle-tools   обновить rs-bundle.txt"
	@echo "  tree-clean     удалить снимки структуры"
	@echo ""
	@echo "Переменные:"
	@echo "  FORCE=1        перекачать всё, даже если файлы уже есть"

all: cbr panel cbr19 rD k y1 moex usd quarterly-all merge-all enrich check

force:
	$(MAKE) all FORCE=1

# --- скачивание -------------------------------------------------------------

cbr:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-fetch -- \
		--registry $(REGISTRY) \
		--out-dir $(DOWNLOADED)

rosstat: fetch-ros01 fetch-ros02

fetch-ros01:
	$(CARGO) run --manifest-path $(MANIFEST) -p rosstat-fetch -- \
		--mode links \
		--html $(DOWNLOADED)/ROS01/housing_prices.html

fetch-ros02:
	$(CARGO) run --manifest-path $(MANIFEST) -p rosstat-fetch -- \
		--mode section \
		--section "Доходы, расходы и сбережения населения" \
		--html $(DOWNLOADED)/ROS02/folder_13397.html

# --- базовая панель ---------------------------------------------------------

panel:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- panel \
		--cbr01 $(DOWNLOADED)/CBR01_households_o.xlsx \
		--cbr02 $(DOWNLOADED)/CBR02_households_b.xlsx \
		--cbr03 $(DOWNLOADED)/CBR03_households_om.xlsx \
		--cbr04 $(DOWNLOADED)/CBR04_households_bm.xlsx \
		--out $(PANEL)

# --- внешние ряды -----------------------------------------------------------

cbr19:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- extract-config \
		--config $(CONFIG) \
		--root $(DATA) \
		--out $(CBR19_LONG)
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- quarterly \
		--in $(CBR19_LONG) \
		--out $(CBR19_Q)

rD:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- extract-config \
		--config $(CONFIG) \
		--root $(DATA) \
		--only rD \
		--out $(RD_LONG)
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- quarterly \
		--in $(RD_LONG) \
		--out $(RD_Q)

k:
	@if [ -f $(K_LONG) ] && [ -z "$(FORCE)" ]; then \
		echo "skip fetch-key-rate: $(K_LONG) exists"; \
	else \
		$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- fetch-key-rate \
			--from 2018-01-01 \
			--to 2026-12-31 \
			--out $(K_LONG); \
	fi
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- quarterly \
		--in $(K_LONG) \
		--out $(K_Q)

y1:
	@if [ -f $(Y1_LONG) ] && [ -z "$(FORCE)" ]; then \
		echo "skip fetch-zcyc: $(Y1_LONG) exists"; \
	else \
		$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- fetch-zcyc \
			--from 2018-01-01 \
			--to 2026-12-31 \
			--chunk-months 6 \
			--out $(Y1_LONG); \
	fi
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- quarterly \
		--in $(Y1_LONG) \
		--out $(Y1_Q)

moex:
	@if [ -f $(MOEX_LONG) ] && [ -z "$(FORCE)" ]; then \
		echo "skip fetch-moex: $(MOEX_LONG) exists"; \
	else \
		$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- fetch-moex \
			--from 2017-01-01 \
			--to 2026-12-31 \
			--out $(MOEX_LONG); \
	fi
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- quarterly \
		--in $(MOEX_LONG) \
		--out $(MOEX_Q)

usd:
	@if [ -f $(USD_LONG) ] && [ -z "$(FORCE)" ]; then \
		echo "skip fetch-usd: $(USD_LONG) exists"; \
	else \
		$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- fetch-usd \
			--from 01.01.2018 \
			--to 31.12.2026 \
			--out $(USD_LONG); \
	fi
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- quarterly \
		--in $(USD_LONG) \
		--out $(USD_Q)

# --- склейка и обогащение ---------------------------------------------------

quarterly-all: cbr19 rD k y1 moex usd

merge-all: quarterly-all
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- merge \
		--panel $(PANEL) \
		--rows $(CBR19_Q),$(RD_Q),$(K_Q),$(Y1_Q),$(MOEX_Q),$(USD_Q) \
		--out $(PANEL) \
		--min-quarter $(MIN_Q) \
		--max-quarter $(MAX_Q)

enrich: merge-all
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- enrich \
		--panel $(PANEL) \
		--out $(PANEL)

check:
	$(CARGO) run --manifest-path $(MANIFEST) -p cbr-panel -- check \
		--panel $(PANEL) \
		--first $(MIN_Q) \
		--last $(MAX_Q)

# --- структура и бандл ------------------------------------------------------

tree: tree-data tree-tools bundle-tools

tree-data:
	$(TREE_SH) $(DATA) > $(TREE_DATA)

tree-tools:
	$(TREE_SH) tools > $(TREE_TOOLS)

bundle-tools:
	$(BUNDLE_SH) tools/rs $(RS_BUNDLE)

tree-clean:
	rm -f $(TREE_DATA) $(TREE_TOOLS) $(RS_BUNDLE)

# --- очистка ----------------------------------------------------------------

clean:
	rm -f $(TMP)/*.xlsx
	rm -f $(CONSTRUCTED)/*.backup.*.xlsx
