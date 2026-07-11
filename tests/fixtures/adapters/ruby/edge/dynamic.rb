class Widget
  attr_reader :name
  attr_accessor :state

  define_method(:computed) do
  end

  def method_missing(sym, *args)
  end

  class << self
    def factory
    end
  end
end
