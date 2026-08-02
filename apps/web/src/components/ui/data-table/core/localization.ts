export interface DataTableLocalization {
  // Selection
  selectAll: string;
  selectRow: string;
  clearSelection: string;
  rowsSelected: (selected: number, total: number) => string;

  // Sorting
  sortByColumnAsc: (column: string) => string;
  sortByColumnDesc: (column: string) => string;
  sortAscending: string;
  sortDescending: string;
  clearSort: string;
  sortedAscending: string;
  sortedDescending: string;

  // Column actions
  columnActions: string;
  hideColumn: string;
  showAllColumns: string;
  pinToLeft: string;
  pinToRight: string;
  unpin: string;
  reorderColumn: string;
  reorderRow: string;
  pinRow: string;
  unpinRow: string;
  resizeColumn: string;

  // Grouping / expansion
  groupByColumn: (column: string) => string;
  ungroupByColumn: (column: string) => string;
  groupedBy: string;
  dropToGroupBy: string;
  expand: string;
  collapse: string;
  expandAll: string;
  collapseAll: string;
  toggleRowExpanded: string;

  // Column visibility
  columnVisibility: string;
  toggleColumnVisibility: string;

  // Filtering
  filterByColumn: (column: string) => string;
  clearFilter: string;
  filterMode: string;
  changeFilterMode: string;
  filterPlaceholder: (column: string) => string;
  showColumnFilters: string;
  hideColumnFilters: string;
  min: string;
  max: string;
  pickDate: string;
  pickDateRange: string;
  /** Labels for each filter mode, keyed by the `FilterMode` string. */
  filterModes: Record<string, string>;

  // Advanced filter panel
  advancedFilters: string;
  advancedFiltersMatchLabel: string;
  advancedFiltersMatchAll: string;
  advancedFiltersMatchAny: string;
  advancedFiltersOf: string;
  advancedFiltersAddRule: string;
  advancedFiltersApply: string;
  advancedFiltersClearAll: string;
  advancedFiltersColumn: string;
  advancedFiltersOperator: string;
  advancedFiltersValue: string;
  advancedFiltersEmpty: string;
  removeFilterRule: string;
  /** Operator labels, keyed by `AdvancedFilterOperator`. */
  advancedFilterOperators: Record<string, string>;

  // Global search
  search: string;
  searchPlaceholder: string;
  clearSearch: string;
  globalFilterMode: string;

  // Density
  toggleDensity: string;
  densityComfortable: string;
  densityCompact: string;
  densitySpacious: string;

  // Full screen
  enterFullscreen: string;
  exitFullscreen: string;

  // Pagination
  rowsPerPage: string;
  paginationRange: (start: number, end: number, total: number) => string;
  goToFirstPage: string;
  goToPreviousPage: string;
  goToNextPage: string;
  goToLastPage: string;
  goToPage: (page: number) => string;

  // Editing / actions
  rowActions: string;
  edit: string;
  save: string;
  cancel: string;
  delete: string;
  create: string;
  createNewRow: string;
  editRow: string;
  required: string;
  copy: string;
  copied: string;
  cellActions: string;

  // Export
  export: string;
  exportCsv: string;
  exportExcel: string;
  exportPdf: string;

  // Empty / loading
  noRecordsToDisplay: string;
  loading: string;
}

export const defaultLocalization: DataTableLocalization = {
  selectAll: "Selecionar tudo",
  selectRow: "Selecionar linha",
  clearSelection: "Limpar seleção",
  rowsSelected: (selected, total) =>
    `${selected} de ${total} linha${total === 1 ? "" : "s"} selecionada${total === 1 ? "" : "s"}`,

  sortByColumnAsc: (column) => `Ordenar por ${column} crescente`,
  sortByColumnDesc: (column) => `Ordenar por ${column} decrescente`,
  sortAscending: "Ordem crescente",
  sortDescending: "Ordem decrescente",
  clearSort: "Limpar ordenação",
  sortedAscending: "Ordenado crescente",
  sortedDescending: "Ordenado decrescente",

  columnActions: "Ações da coluna",
  hideColumn: "Ocultar coluna",
  showAllColumns: "Mostrar todas as colunas",
  pinToLeft: "Fixar à esquerda",
  pinToRight: "Fixar à direita",
  unpin: "Desafixar",
  reorderColumn: "Reordenar coluna",
  reorderRow: "Reordenar linha",
  pinRow: "Fixar linha",
  unpinRow: "Desafixar linha",
  resizeColumn: "Redimensionar coluna",

  groupByColumn: (column) => `Agrupar por ${column}`,
  ungroupByColumn: (column) => `Desagrupar por ${column}`,
  groupedBy: "Agrupado por",
  dropToGroupBy: "Arraste uma coluna aqui para agrupar",
  expand: "Expandir",
  collapse: "Recolher",
  expandAll: "Expandir tudo",
  collapseAll: "Recolher tudo",
  toggleRowExpanded: "Alternar expansão da linha",

  columnVisibility: "Visibilidade das colunas",
  toggleColumnVisibility: "Alternar visibilidade da coluna",

  filterByColumn: (column) => `Filtrar por ${column}`,
  clearFilter: "Limpar filtro",
  filterMode: "Modo de filtro",
  changeFilterMode: "Alterar modo de filtro",
  filterPlaceholder: (column) => `Filtrar ${column}…`,
  showColumnFilters: "Mostrar filtros",
  hideColumnFilters: "Ocultar filtros",
  min: "Mín",
  max: "Máx",
  pickDate: "Escolha uma data",
  pickDateRange: "Escolha um período",
  filterModes: {
    fuzzy: "Aproximado",
    contains: "Contém",
    startsWith: "Começa com",
    endsWith: "Termina com",
    equals: "Igual",
    notEquals: "Diferente",
    empty: "Vazio",
    notEmpty: "Não vazio",
    between: "Entre (exclusivo)",
    betweenInclusive: "Entre (inclusivo)",
    greaterThan: "Maior que",
    greaterThanOrEqualTo: "Maior ou igual a",
    lessThan: "Menor que",
    lessThanOrEqualTo: "Menor ou igual a",
    before: "Antes de",
    after: "Depois de",
    betweenDates: "Entre",
    equalsString: "Igual",
    arrIncludesSome: "Inclui",
    equalsBool: "Igual",
  },

  advancedFilters: "Filtros avançados",
  advancedFiltersMatchLabel: "Corresponder",
  advancedFiltersMatchAll: "Todos",
  advancedFiltersMatchAny: "Qualquer",
  advancedFiltersOf: "das regras abaixo",
  advancedFiltersAddRule: "Adicionar filtro",
  advancedFiltersApply: "Aplicar",
  advancedFiltersClearAll: "Limpar tudo",
  advancedFiltersColumn: "Coluna",
  advancedFiltersOperator: "Operador",
  advancedFiltersValue: "Valor",
  advancedFiltersEmpty: "Nenhum filtro ainda. Adicione um para começar.",
  removeFilterRule: "Remover filtro",
  advancedFilterOperators: {
    contains: "contém",
    notContains: "não contém",
    startsWith: "começa com",
    endsWith: "termina com",
    equals: "igual a",
    notEquals: "diferente de",
    isEmpty: "está vazio",
    isNotEmpty: "não está vazio",
    greaterThan: "maior que",
    greaterThanOrEqual: "maior ou igual a",
    lessThan: "menor que",
    lessThanOrEqual: "menor ou igual a",
    between: "está entre",
  },

  search: "Buscar",
  searchPlaceholder: "Buscar…",
  clearSearch: "Limpar busca",
  globalFilterMode: "Modo de busca",

  toggleDensity: "Alternar densidade",
  densityComfortable: "Confortável",
  densityCompact: "Compacto",
  densitySpacious: "Espaçoso",

  enterFullscreen: "Entrar em tela cheia",
  exitFullscreen: "Sair da tela cheia",

  rowsPerPage: "Linhas por página",
  paginationRange: (start, end, total) => `${start}-${end} de ${total}`,
  goToFirstPage: "Ir para a primeira página",
  goToPreviousPage: "Ir para a página anterior",
  goToNextPage: "Ir para a próxima página",
  goToLastPage: "Ir para a última página",
  goToPage: (page) => `Ir para a página ${page}`,

  rowActions: "Ações da linha",
  edit: "Editar",
  save: "Salvar",
  cancel: "Cancelar",
  delete: "Excluir",
  create: "Criar",
  createNewRow: "Criar nova linha",
  editRow: "Editar linha",
  required: "Obrigatório",
  copy: "Copiar",
  copied: "Copiado",
  cellActions: "Ações da célula",

  export: "Exportar",
  exportCsv: "Exportar para CSV",
  exportExcel: "Exportar para Excel",
  exportPdf: "Exportar para PDF",

  noRecordsToDisplay: "Nenhum registro para exibir",
  loading: "Carregando…",
};
