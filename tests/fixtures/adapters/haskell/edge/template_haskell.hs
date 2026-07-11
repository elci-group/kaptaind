{-# LANGUAGE TemplateHaskell #-}

real :: Int -> Int
real x = x + 1

$(makeLenses ''Config)
