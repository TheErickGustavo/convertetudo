# ConverteTudo

Conversor local de documentos. O arquivo é lido e escrito nesta máquina. Nada é enviado para fora.

Abra a interface, solte um ou mais arquivos, escolha a saída e baixe. A folha à direita é a prova do texto, antes do download.

## Formatos

Leitura: EPUB, HTML, DOCX, ODT, RTF, PDF, TXT, Markdown, FB2, MOBI (PalmDOC), CSV, XLSX, ODS, XPS, ABW, LaTeX.

Escrita: TXT, Markdown, HTML, RTF, DOCX, ODT, PDF, EPUB, FB2, CSV, XLSX, LaTeX, JSON.

PDF só entra se tiver camada de texto. Página escaneada não é convertida: esta versão não faz OCR. MOBI compactado com Huff/CDIC é recusado, com o motivo. Arquivo com senha também.

## Linha de comando

```
convertetudo
convertetudo serve --port 4173
convertetudo convert livro.epub -t txt -o livro.txt
convertetudo convert *.docx -t pdf -d saida/
convertetudo selftest
```

Com Rust instalado, `cargo run -- serve --port 4173`. O binário escuta em `0.0.0.0`.
