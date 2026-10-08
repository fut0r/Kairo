<#
.SYNOPSIS
  Draws the pictures the Windows installers show, from the app icon.

.DESCRIPTION
  Run it again whenever icons/icon.png changes:

      powershell -ExecutionPolicy Bypass -File installer/make-images.ps1

  Both installers want 24-bit BMP files at exactly these sizes. They draw
  their own black text over the two .msi pictures, so those stay white
  wherever that text goes.

      sidebar.bmp     164 x 314   setup.exe, welcome and finish pages
      header.bmp      150 x 57    setup.exe, top left of the other pages
      wix-dialog.bmp  493 x 312   .msi, welcome and finish pages
      wix-banner.bmp  493 x 58    .msi, top of the other pages
#>
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$icon = [Drawing.Image]::FromFile((Join-Path $PSScriptRoot '..\icons\icon.png'))

# The app's dark theme, from src/styles/tokens.css.
$graphite = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#1b1b1b'))
$text = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#e8e8e8'))
$accent = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#4cc2ff'))

function New-Picture([int]$width, [int]$height, [string]$name, [scriptblock]$draw) {
    $bitmap = New-Object Drawing.Bitmap $width, $height, ([Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [Drawing.Graphics]::FromImage($bitmap)
    $g.InterpolationMode = 'HighQualityBicubic'
    $g.PixelOffsetMode = 'HighQuality'
    $g.SmoothingMode = 'AntiAlias'
    # Greyscale smoothing: the installers stretch these on high-DPI screens,
    # and ClearType fringes do not survive that.
    $g.TextRenderingHint = 'AntiAliasGridFit'
    $g.Clear([Drawing.Color]::White)
    & $draw $g
    $g.Dispose()
    $bitmap.Save((Join-Path $PSScriptRoot $name), [Drawing.Imaging.ImageFormat]::Bmp)
    $bitmap.Dispose()
    "{0,-15} {1} x {2}" -f $name, $width, $height
}

function Write-Name($g, [int]$pixels, [Drawing.RectangleF]$box, [string]$align) {
    $font = New-Object Drawing.Font 'Segoe UI Semibold', $pixels, ([Drawing.FontStyle]::Regular), ([Drawing.GraphicsUnit]::Pixel)
    $format = New-Object Drawing.StringFormat
    $format.Alignment = $align
    $format.LineAlignment = 'Center'
    $g.DrawString('KairoDB', $font, $text, $box, $format)
    $font.Dispose()
}

# The icon above the name, centred in a graphite column.
function Write-Column($g, [int]$width, [int]$height) {
    $g.FillRectangle($graphite, 0, 0, $width, $height)
    $size = 104
    $g.DrawImage($icon, [int](($width - $size) / 2), 62, $size, $size)
    Write-Name $g 24 (New-Object Drawing.RectangleF 0, 178, $width, 40) 'Center'
    $g.FillRectangle($accent, 0, $height - 3, $width, 3)
}

New-Picture 164 314 'sidebar.bmp' { param($g) Write-Column $g 164 314 }

New-Picture 150 57 'header.bmp' {
    param($g)
    $g.FillRectangle($graphite, 0, 0, 150, 57)
    $g.DrawImage($icon, 10, 10, 37, 37)
    Write-Name $g 19 (New-Object Drawing.RectangleF 54, 0, 96, 55) 'Near'
}

# Text starts 180 px in on these pages; the column stops short of it.
New-Picture 493 312 'wix-dialog.bmp' { param($g) Write-Column $g 164 312 }

# Titles and descriptions run from the left to about 410 px.
New-Picture 493 58 'wix-banner.bmp' { param($g) $g.DrawImage($icon, 437, 7, 44, 44) }

$icon.Dispose()
