param(
    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing

function Export-TransparentCrop {
    param(
        [Parameter(Mandatory)] [string]$SourcePath,
        [Parameter(Mandatory)] [System.Drawing.Rectangle]$Region,
        [Parameter(Mandatory)] [string]$OutputPath,
        [int]$TargetHeight = 0,
        [int]$TargetWidth = 0
    )

    $source = [System.Drawing.Bitmap]::FromFile($SourcePath)
    try {
        $canvas = [System.Drawing.Bitmap]::new($Region.Width, $Region.Height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($canvas)
            $attributes = New-Object System.Drawing.Imaging.ImageAttributes
            try {
                $graphics.Clear([System.Drawing.Color]::Transparent)
                $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
                $attributes.SetColorKey(
                    [System.Drawing.Color]::FromArgb(235, 235, 235),
                    [System.Drawing.Color]::FromArgb(255, 255, 255)
                )
                $destination = [System.Drawing.Rectangle]::new(0, 0, $Region.Width, $Region.Height)
                $graphics.DrawImage(
                    $source,
                    $destination,
                    $Region.X,
                    $Region.Y,
                    $Region.Width,
                    $Region.Height,
                    [System.Drawing.GraphicsUnit]::Pixel,
                    $attributes
                )
            }
            finally {
                $attributes.Dispose()
                $graphics.Dispose()
            }

            $left = $canvas.Width
            $top = $canvas.Height
            $right = -1
            $bottom = -1
            for ($y = 0; $y -lt $canvas.Height; $y++) {
                for ($x = 0; $x -lt $canvas.Width; $x++) {
                    if ($canvas.GetPixel($x, $y).A -eq 0) { continue }
                    if ($x -lt $left) { $left = $x }
                    if ($x -gt $right) { $right = $x }
                    if ($y -lt $top) { $top = $y }
                    if ($y -gt $bottom) { $bottom = $y }
                }
            }

            if ($right -lt $left -or $bottom -lt $top) {
                throw "No foreground pixels found in $SourcePath"
            }

            $trimWidth = $right - $left + 1
            $trimHeight = $bottom - $top + 1
            $trimRegion = [System.Drawing.Rectangle]::new($left, $top, $trimWidth, $trimHeight)
            $trimmed = $canvas.Clone($trimRegion, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
            try {
                if ($TargetWidth -gt 0) {
                    $width = $TargetWidth
                    $height = [Math]::Max(1, [int][Math]::Round($trimmed.Height * $TargetWidth / $trimmed.Width))
                }
                elseif ($TargetHeight -gt 0) {
                    $height = $TargetHeight
                    $width = [Math]::Max(1, [int][Math]::Round($trimmed.Width * $TargetHeight / $trimmed.Height))
                }
                else {
                    $width = $trimmed.Width
                    $height = $trimmed.Height
                }

                $output = [System.Drawing.Bitmap]::new($width, $height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
                try {
                    $outputGraphics = [System.Drawing.Graphics]::FromImage($output)
                    try {
                        $outputGraphics.Clear([System.Drawing.Color]::Transparent)
                        $outputGraphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
                        $outputGraphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
                        $outputGraphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                        $outputGraphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                        $outputGraphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
                        $outputGraphics.DrawImage($trimmed, 0, 0, $width, $height)
                    }
                    finally {
                        $outputGraphics.Dispose()
                    }
                    $output.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
                }
                finally {
                    $output.Dispose()
                }
            }
            finally {
                $trimmed.Dispose()
            }
        }
        finally {
            $canvas.Dispose()
        }
    }
    finally {
        $source.Dispose()
    }
}

function Export-DarkVariant {
    param(
        [Parameter(Mandatory)] [string]$SourcePath,
        [Parameter(Mandatory)] [string]$OutputPath
    )

    $source = [System.Drawing.Bitmap]::FromFile($SourcePath)
    try {
        $output = [System.Drawing.Bitmap]::new($source.Width, $source.Height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            for ($y = 0; $y -lt $source.Height; $y++) {
                for ($x = 0; $x -lt $source.Width; $x++) {
                    $pixel = $source.GetPixel($x, $y)
                    if ($pixel.A -eq 0) {
                        $output.SetPixel($x, $y, [System.Drawing.Color]::Transparent)
                        continue
                    }

                    $isBrandBlue = ($pixel.B - $pixel.R -gt 80) -and ($pixel.B - $pixel.G -gt 80)
                    if ($isBrandBlue) {
                        $output.SetPixel($x, $y, $pixel)
                        continue
                    }

                    $brightness = [int](0.2126 * $pixel.R + 0.7152 * $pixel.G + 0.0722 * $pixel.B)
                    $replacement = if ($brightness -gt 90) {
                        [System.Drawing.Color]::FromArgb($pixel.A, 188, 199, 217)
                    }
                    else {
                        [System.Drawing.Color]::FromArgb($pixel.A, 244, 247, 252)
                    }
                    $output.SetPixel($x, $y, $replacement)
                }
            }
            $output.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally {
            $output.Dispose()
        }
    }
    finally {
        $source.Dispose()
    }
}

$assets = Join-Path $RepositoryRoot "assets"
$markPath = Join-Path $assets "codo_mark.png"
$wordmarkPath = Join-Path $assets "codo_wordmark.png"
$wordmarkDarkPath = Join-Path $assets "codo_wordmark_dark.png"
$signaturePath = Join-Path $assets "codo_signature.png"
$signatureDarkPath = Join-Path $assets "codo_signature_dark.png"
$appIconPath = Join-Path $assets "codo_app_icon.png"
$faviconPath = Join-Path $assets "codo_favicon.png"
$nativeMarkPath = Join-Path ([System.IO.Path]::GetTempPath()) "codo-mark-native-$PID.png"

Export-TransparentCrop `
    -SourcePath (Join-Path $assets "codo_big.png") `
    -Region ([System.Drawing.Rectangle]::new(300, 300, 650, 360)) `
    -OutputPath $markPath `
    -TargetHeight 256

Export-TransparentCrop `
    -SourcePath (Join-Path $assets "codo_big.png") `
    -Region ([System.Drawing.Rectangle]::new(300, 300, 650, 360)) `
    -OutputPath $nativeMarkPath `
    -TargetHeight 512

Export-TransparentCrop `
    -SourcePath (Join-Path $assets "codo_brand_ask.png") `
    -Region ([System.Drawing.Rectangle]::new(680, 300, 850, 270)) `
    -OutputPath $wordmarkPath `
    -TargetWidth 600

Export-TransparentCrop `
    -SourcePath (Join-Path $assets "codo_brand_ask.png") `
    -Region ([System.Drawing.Rectangle]::new(150, 280, 1375, 380)) `
    -OutputPath $signaturePath `
    -TargetWidth 800

Export-DarkVariant -SourcePath $wordmarkPath -OutputPath $wordmarkDarkPath
Export-DarkVariant -SourcePath $signaturePath -OutputPath $signatureDarkPath

$mark = [System.Drawing.Bitmap]::FromFile($nativeMarkPath)
try {
    $icon = [System.Drawing.Bitmap]::new(1024, 1024, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($icon)
        try {
            $graphics.Clear([System.Drawing.Color]::Transparent)
            $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
            $iconWidth = 840
            $iconHeight = [int][Math]::Round($mark.Height * $iconWidth / $mark.Width)
            $graphics.DrawImage($mark, [int]((1024 - $iconWidth) / 2), [int]((1024 - $iconHeight) / 2), $iconWidth, $iconHeight)
        }
        finally {
            $graphics.Dispose()
        }
        $icon.Save($appIconPath, [System.Drawing.Imaging.ImageFormat]::Png)

        $favicon = [System.Drawing.Bitmap]::new(64, 64, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $faviconGraphics = [System.Drawing.Graphics]::FromImage($favicon)
            try {
                $faviconGraphics.Clear([System.Drawing.Color]::Transparent)
                $faviconGraphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
                $faviconGraphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $faviconGraphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $faviconGraphics.DrawImage($icon, 0, 0, 64, 64)
            }
            finally {
                $faviconGraphics.Dispose()
            }
            $favicon.Save($faviconPath, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally {
            $favicon.Dispose()
        }
    }
    finally {
        $icon.Dispose()
    }
}
finally {
    $mark.Dispose()
    Remove-Item -LiteralPath $nativeMarkPath -Force -ErrorAction SilentlyContinue
}

Write-Output "Generated CoDo brand assets in $assets"
