using System.Runtime.InteropServices;

namespace WebCheck;

[StructLayout(LayoutKind.Sequential, Size = 1)]
internal struct TypErrList
{
	public const int OK = 0;

	public const int ДоступПоЭтомуФискальномуНомеруЗапрещен = 1;

	public const int ПодключенДругойФискальныйНомер = 2;

	public const int НеправильныйФорматФискальногоНомера = 3;

	public const int НетПодключеннойБазы = 4;

	public const int НетТакогоОператора = 5;

	public const int ОшибкаПолученияДанныхTaxObjectsFN = 6;

	public const int НеСмогОткрытьСмену = 7;

	public const int НетОткрытойСмены = 8;

	public const int ОшибкаОбработкиTAXES = 9;

	public const int НеправильныйФорматXMLчека = 10;

	public const int ОшибкаОбработкиЧека = 11;

	public const int ОшибкаПриОбработкеТоваровXML = 12;

	public const int НеСовпалаСуммаЧека = 13;

	public const int ОшибкаЗаписиТоваровБД = 14;

	public const int НеМогуПолучитьЛокальныйНомерЧека = 15;

	public const int ОшибкаЗаписиЧека = 16;

	public const int ОшибкаОбработкиPayForms = 17;

	public const int ОшибкаФормированияZXотчета = 18;

	public const int ОшибочныйТипОпарацииВчеке = 19;

	public const int ОшибкаОбработкиЧекаВносаВыноса = 20;

	public const int ОшибкаПоученияXMLчека = 21;

	public const int ОшибкаТакогоНомераЧекаНет = 22;

	public const int ТакойКомандыНет = 23;

	public const int ОшибкаКонтекстногоНомера = 24;

	public const int ОшибкаОтправкиЧека = 25;

	public const int ОтправленныйЧекНеПринят = 26;

	public const int ОтправленныйZотчетНеПринят = 27;

	public const int НеПравельныйФорматЛокальногоНомераЧека = 28;

	public const int ОшибкаЗаписиЧекаДействия = 29;

	public const int ОшибкаПолученияПредыдущегоМака = 30;

	public const int ОшибкаПолученияПоследнегоЧека = 31;

	public const int ОфлайнРежим = 32;

	public const int НетИнтернетаЗапрещенОфлайнРежим = 33;

	public const int НетИнтернетаБесплатнаяВерсия = 34;

	public const int ОшибкаПолученияОфлайнНомера = 35;

	public const int ОшибкаПолученияОфлайнНомеров = 36;

	public const int ОшибкаВключенияРежимаОнлайн = 37;

	public const int ОшибкаПолученияОфлайнЧекаИзБазы = 38;

	public const int ОшибкаОтправкиОфлайнЧека = 39;

	public const int ОшибкаИзмененияЗаписиТалицыKsef = 40;

	public const int ОшибкаВключенияРежимаОффлайн = 41;

	public const int ПревышенЛимитОффлайнРежима = 42;

	public const int ДваТипаАкцизаВодномЧеке = 43;

	public const int АвтоматическоеВключениеОфлайнРежимаЗапрещено = 44;

	public const int СуммаЧекаБольшеЛимита = 45;

	public const int ВремяСменыИстекло = 46;

	public const int НеХватаетДенегВкассе = 47;

	public const int ОшибкаРасшифровкиРезервныхНомеров = 48;

	public const int НетФайлаКлюча = 49;

	public const int ВключенОфлайнРежим = 50;

	public const int ОшибкаДобавлениеНовогоПлатежа = 51;

	public const int ОшибкаДанныйНомерИспользуется = 52;

	public const int ОшибкаЗаписиДекодированнойСтрокиВфайл = 53;

	public const int ОшибкаДекодирования = 54;

	public const int РаботаетАвтоматическийРежимОфлайн = 55;

	public const int ОшибкаПолученияБлокировок = 56;

	public const int ОшибкаУстановкиБлокировки = 57;

	public const int БлокировкаУжеЕсть = 58;

	public const int ОшибкаИзмененияЗаписиТалицыSessions = 59;

	public const int ОшибочныеСимволы = 60;

	public const int ОшибкаДвеБлокировки = 61;

	public const int ОшибкаНетТакогоПлатежа = 62;

	public const int ОшибкаПолученияДанныхДляОтправкиПочты = 63;

	public const int СуммаСдачиБольшеСуммыНала = 64;

	public const int ПроверкаСертификата = 65;

	public const int СрокДействияСертификатаИстекает = 66;

	public const int ОшибкаСервераПодписиДляРобота = 67;

	public const int ОшибкаФормированияПереодическогоОтчета = 68;

	public const int ОшибкаDNS = 69;

	public const int ОшибкаДопустимогоНала = 70;

	public const int ОшибкаПодписиФайла = 71;

	public const int ОшибкаПовторныйИндексПлатежа = 72;

	public const int БлокировкаЗакрытияОфлайн = 73;

	public const int НеобходимоОтключитьФН = 74;

	public const int НетТакогоФайла = 75;

	public const int ОшибкаПриОбработкеНалоговXML = 76;

	public const int ОшибкаБольшоеРасхождениеВремени = 77;

	public const int ОшибкаПереименованияПлатежа = 78;

	public const int ОшибкаНомерСменыНеЧисло = 79;

	public const int ОшибкаПодключенияФН = 80;

	public const int ОшибкаПолученияСпискаЧеков = 81;

	public const int ОшибкаПолученияСпискаСмен = 82;

	public const int ОшибкаДобавленияОператора = 83;

	public const int ОшибкаОффлайнрежимЗакрыт = 84;

	public const int ОшибкаСозданияНовогоПРРО = 85;

	public const int ОшибкаФорматаДаты = 86;

	public const int ОшибкаФормированияПериодическогоОтчета = 87;

	public const int ОшибкаКонтроляUID = 88;

	public const int ОшибкаПервыйДокументНеМожетБытьОфлайн = 89;

	public const int ОшибкаИзмененийВтаблицеFNS = 90;

	public const int ОшибкаОтправкиВайбер = 91;

	public const int ОшибкаЗапросаВайбер = 92;

	public const int ОшибкаСозданиеНовогоПРРО = 93;

	public const int ОшибкаEPZtoCash = 94;

	public const int ТехническоеОткрытиеСмены = 95;

	public const int ОшибкаИзмененияЗаписиТалицыOPERATORNAME = 96;

	public const int ТехническоеОткрытиеОффлайн = 97;

	public const int ОшибкаЧекНеНайден = 98;

	public const int ОшибкаОтправкиPDF = 99;

	public const int ОшибкаПолученияОтветаПриОтправкеЧека = 100;

	public const int ОшибкаСистемногоВремени = 101;

	public const int ОшибкаОкругленияСуммыЧека = 102;

	public const int ОшибкаТерминала = 103;

	public const int ОшибкаЗакрытойСмены = 104;

	public const int ОшибкаОткрытаСмена = 105;

	public const int ОшибкаИзмененияЗаписиТалицы = 106;

	public const int ОшибкаПолученияВерсииОбновления = 107;

	public const int ОшибкаРегиональныхНастроек = 1000;

	public const int ОшибкаТакогоФискальногоНомераНет = 1001;

	public const int ОшибкаТакогоОператораНет = 1002;

	public const int ОшибкаОткрытоНесколькоСмен = 1003;

	public const int ОшибкаXMLНеправильныйФормат = 1004;

	public const int ОшибкаXMLНеправильноСформированОтвет = 1005;

	public const int ОшибкаОткрытаДругаяСмена = 1006;

	public const int НеМогуЗакрытьТекущуюСмену = 1007;

	public const int НетИнформацииОналогах = 1008;

	public const int НетИнформацииОплатежах = 1009;

	public const int ОшибкаПолученияКлюча = 1010;

	public const int ОшибкаПриПодписиФайла = 1011;

	public const int ОшибкаИнициализацииПроцедурыПодписи = 1012;

	public const int ОшибкаВторойПоток = 1013;

	public const int ОшибкаТаблицыОператоров = 1014;

	public const int БазаЗаблокированна = 1015;

	public const int ПовторныйЛокальныйЗапуск = 1016;

	public const int ОтрицательноеЧисло = 1017;

	public const int СертификатОтозван = 1018;
}
