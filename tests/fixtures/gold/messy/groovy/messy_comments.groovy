#!/usr/bin/env groovy
// class FakeOne {
//     def hacked() {}
// }

/*
interface IFake {
    def stolen()
}
*/

def text = '''
class InString {
    def fake() {}
}
'''

class Real {
    def genuine() {
        return 1
    }
}
