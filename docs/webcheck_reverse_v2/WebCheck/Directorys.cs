using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Directorys
{
	private int TaxS;

	private string[,] Tax;

	private int PayS;

	private string[,] Pay;

	private const int TempsTaxs = 9;

	private string[,] ReportTax;

	private string[,] ReportPay;

	public string Ni;

	public string No;

	public string SMI;

	public string SMO;

	public string ReportsSum
	{
		get
		{
			if (((x > 9) | (y > TaxS)) || x < 0 || y < 1)
			{
				return "";
			}
			return ReportTax[x, y];
		}
		set
		{
			if (!(((x > 9) | (y > TaxS)) || x < 0 || y < 1))
			{
				ReportTax[x, y] = value;
			}
		}
	}

	public double TXSM
	{
		get
		{
			if ((i < 1) | (i > TaxS))
			{
				return -1.0;
			}
			return All.StrToDouble(ReportTax[6, i]);
		}
		set
		{
			if (!((i < 1) | (i > TaxS)))
			{
				ReportTax[6, i] = value.ToString();
			}
		}
	}

	public string TaxTemp7
	{
		get
		{
			if ((i < 1) | (i > TaxS))
			{
				return "";
			}
			return ReportTax[7, i];
		}
		set
		{
			if (!((i < 1) | (i > TaxS)))
			{
				ReportTax[7, i] = value;
			}
		}
	}

	public string TaxTemp8
	{
		get
		{
			if ((i < 1) | (i > TaxS))
			{
				return "";
			}
			return ReportTax[8, i];
		}
		set
		{
			if (!((i < 1) | (i > TaxS)))
			{
				ReportTax[8, i] = value;
			}
		}
	}

	public string TaxTemp9
	{
		get
		{
			if ((i < 1) | (i > TaxS))
			{
				return "";
			}
			return ReportTax[9, i];
		}
		set
		{
			if (!((i < 1) | (i > TaxS)))
			{
				ReportTax[9, i] = value;
			}
		}
	}

	public string ReportsPay
	{
		get
		{
			return ReportPay[x, y];
		}
		set
		{
			ReportPay[x, y] = value;
		}
	}

	public string SumPay
	{
		get
		{
			if (index > PayS)
			{
				return "0";
			}
			if (index < 2)
			{
				return All.Bablo(All.StrToDouble(Pay[2, 0]) + All.StrToDouble(Pay[2, 1]));
			}
			return Pay[2, index];
		}
		set
		{
			if (index <= PayS)
			{
				Pay[2, index] = value;
			}
		}
	}

	public double SumTax
	{
		get
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				return All.StrToDouble(Tax[3, index]);
			}
			return -1.0;
		}
		set
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				Tax[3, index] = value.ToString();
			}
		}
	}

	public double Sum
	{
		get
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				return All.StrToDouble(Tax[4, index]);
			}
			return -1.0;
		}
		set
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				Tax[4, index] = value.ToString();
			}
		}
	}

	public int PayN => PayS;

	public int TaxN => TaxS;

	public string TaxABCtoNum => Tax[5, index];

	public string TaxName
	{
		get
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				return Tax[0, index];
			}
			return "";
		}
	}

	public string TaxEXCISE
	{
		get
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				return Tax[1, index];
			}
			return "";
		}
	}

	public string TaxPRC
	{
		get
		{
			if ((index > 0) & (index < checked(TaxS + 1)))
			{
				return Tax[2, index];
			}
			return "";
		}
	}

	public string PayName
	{
		get
		{
			if (index == 0)
			{
				return Pay[0, 1];
			}
			if ((index > 0) & (index < checked(PayS + 1)))
			{
				return Pay[0, index];
			}
			return "";
		}
	}

	public string PayISCASH
	{
		get
		{
			if (index == 0)
			{
				index = 1;
			}
			if ((index > 0) & (index < checked(PayS + 1)))
			{
				string left = Pay[1, index];
				if (Operators.CompareString(left, "1", TextCompare: false) == 0)
				{
					return "0";
				}
				if (Operators.CompareString(left, "102", TextCompare: false) == 0)
				{
					return "2";
				}
				return "1";
			}
			return "";
		}
	}

	public string PayISCASHname
	{
		get
		{
			if (index == 0)
			{
				index = 1;
			}
			if ((index > 0) & (index < checked(PayS + 1)))
			{
				string left = Pay[1, index];
				if (Operators.CompareString(left, "1", TextCompare: false) == 0)
				{
					return "готівка";
				}
				if (Operators.CompareString(left, "102", TextCompare: false) == 0)
				{
					return "інше";
				}
				return "безготівка";
			}
			return "";
		}
	}

	public int PayABCtoNum
	{
		get
		{
			int payS = PayS;
			for (int i = 0; i <= payS; i = checked(i + 1))
			{
				string left = Strings.Replace(Pay[0, i].Trim().ToLower(), "&#39;", "'");
				Name = Name.Trim().ToLower();
				Name = Strings.Replace(Name, "&#39;", "'");
				if (Operators.CompareString(left, Name, TextCompare: false) == 0)
				{
					if (i == 1)
					{
						i = 0;
					}
					return i;
				}
			}
			return -1;
		}
	}

	public Directorys()
	{
		TaxS = 0;
		Tax = new string[6, 1];
		PayS = 0;
		Pay = new string[3, 1];
		ReportTax = new string[10, 1];
		ReportPay = new string[3, 181];
		Ni = "";
		No = "";
		SMI = "";
		SMO = "";
	}

	public TypErr StartLoad()
	{
		TypErr typErr = default(TypErr);
		typErr.errStr = "";
		typErr.errCode = 0;
		typErr = LoadPay();
		TypErr result;
		checked
		{
			if (typErr.errCode > 0)
			{
				result = typErr;
			}
			else
			{
				typErr = LoadTaxObjeckt();
				if (typErr.errCode > 0)
				{
					result = typErr;
				}
				else
				{
					try
					{
						int num = 0;
						num++;
						ref string[,] tax = ref Tax;
						tax = (string[,])Utils.CopyArray(tax, new string[6, num + 1]);
						Tax[0, num] = "А";
						Tax[1, num] = "0";
						Tax[2, num] = "20";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax2 = ref Tax;
						tax2 = (string[,])Utils.CopyArray(tax2, new string[6, num + 1]);
						Tax[0, num] = "Б";
						Tax[1, num] = "0";
						Tax[2, num] = "0";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax3 = ref Tax;
						tax3 = (string[,])Utils.CopyArray(tax3, new string[6, num + 1]);
						Tax[0, num] = "В";
						Tax[1, num] = "0";
						Tax[2, num] = "7";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax4 = ref Tax;
						tax4 = (string[,])Utils.CopyArray(tax4, new string[6, num + 1]);
						Tax[0, num] = "ГА";
						Tax[1, num] = "5";
						Tax[2, num] = "20";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax5 = ref Tax;
						tax5 = (string[,])Utils.CopyArray(tax5, new string[6, num + 1]);
						Tax[0, num] = "ГБ";
						Tax[1, num] = "5";
						Tax[2, num] = "0";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax6 = ref Tax;
						tax6 = (string[,])Utils.CopyArray(tax6, new string[6, num + 1]);
						Tax[0, num] = "ДА";
						Tax[1, num] = "7.5";
						Tax[2, num] = "20";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax7 = ref Tax;
						tax7 = (string[,])Utils.CopyArray(tax7, new string[6, num + 1]);
						Tax[0, num] = "ДБ";
						Tax[1, num] = "7.5";
						Tax[2, num] = "0";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax8 = ref Tax;
						tax8 = (string[,])Utils.CopyArray(tax8, new string[6, num + 1]);
						Tax[0, num] = "Е";
						Tax[1, num] = "0";
						Tax[2, num] = "0";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax9 = ref Tax;
						tax9 = (string[,])Utils.CopyArray(tax9, new string[6, num + 1]);
						Tax[0, num] = "Ж";
						Tax[1, num] = "0";
						Tax[2, num] = "0";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax10 = ref Tax;
						tax10 = (string[,])Utils.CopyArray(tax10, new string[6, num + 1]);
						Tax[0, num] = "З";
						Tax[1, num] = "0";
						Tax[2, num] = "0";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						num++;
						ref string[,] tax11 = ref Tax;
						tax11 = (string[,])Utils.CopyArray(tax11, new string[6, num + 1]);
						Tax[0, num] = "К";
						Tax[1, num] = "0";
						Tax[2, num] = "14";
						Tax[5, num] = ABCtoNUM(Tax[0, num]);
						TaxS = num;
						if (TaxS < 1)
						{
							typErr.errCode = 1008;
							typErr.errStr = "Ошибка! Нет информации о налогах. Таблица TAXES.";
							result = typErr;
							goto IL_05d8;
						}
						ReportTax = new string[10, TaxS + 1];
					}
					catch (Exception ex)
					{
						ProjectData.SetProjectError(ex);
						Exception ex2 = ex;
						typErr.errStr = "Ошибка обработки данных талицы TAXES";
						typErr.errCode = 9;
						result = typErr;
						ProjectData.ClearProjectError();
						goto IL_05d8;
					}
					result = typErr;
				}
			}
			goto IL_05d8;
		}
		IL_05d8:
		return result;
	}

	public void ZeroReport()
	{
		int taxS = TaxS;
		checked
		{
			int i;
			for (i = 0; i <= taxS; i++)
			{
				int num = 0;
				do
				{
					ReportTax[num, i] = "";
					num++;
				}
				while (num <= 9);
			}
			i = 0;
			do
			{
				int num = 0;
				do
				{
					ReportPay[num, i] = "";
					num++;
				}
				while (num <= 2);
				i++;
			}
			while (i <= 180);
			Ni = "";
			No = "";
			SMI = "";
			SMO = "";
		}
	}

	public string ABCtoNUM(string L)
	{
		L = L.ToLower().Trim();
		return L switch
		{
			"а" => "1", 
			"a" => "1", 
			"б" => "2", 
			"b" => "2", 
			"в" => "3", 
			"v" => "3", 
			"w" => "3", 
			"га" => "4", 
			"ga" => "4", 
			"гa" => "4", 
			"gа" => "4", 
			"гб" => "5", 
			"gb" => "5", 
			"гb" => "5", 
			"gб" => "5", 
			"да" => "6", 
			"da" => "6", 
			"дa" => "6", 
			"dа" => "6", 
			"дб" => "7", 
			"dб" => "7", 
			"дb" => "7", 
			"db" => "7", 
			"е" => "8", 
			"ё" => "8", 
			"e" => "8", 
			"ж" => "9", 
			"j" => "9", 
			"з" => "10", 
			"z" => "10", 
			"к" => "11", 
			"k" => "11", 
			"л" => "12", 
			"l" => "12", 
			_ => "0", 
		};
	}

	public string NUMtoABC(string L)
	{
		L = L.ToLower().Trim();
		return L switch
		{
			"1" => "А", 
			"2" => "Б", 
			"3" => "В", 
			"4" => "ГА", 
			"5" => "ГБ", 
			"6" => "ДА", 
			"7" => "ДБ", 
			"8" => "Е", 
			"9" => "Ж", 
			"10" => "З", 
			"11" => "К", 
			"12" => "Л", 
			_ => "Я", 
		};
	}

	private TypErr LoadPay()
	{
		TypErr typErr = default(TypErr);
		typErr.errStr = "";
		typErr.errCode = 0;
		TypErr result;
		checked
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM PayForms";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				int num = 0;
				Pay[0, num] = "Готівка";
				Pay[1, num] = "0";
				Pay[2, num] = "0";
				while (sQLiteDataReader.Read())
				{
					num++;
					ref string[,] pay = ref Pay;
					pay = (string[,])Utils.CopyArray(pay, new string[3, num + 1]);
					Pay[0, num] = sQLiteDataReader[1].ToString();
					Pay[1, num] = sQLiteDataReader[2].ToString();
					Pay[2, num] = "0";
					if (Pay[0, num].Length > 18)
					{
						All.PayU = "•";
						All.PayD = "•";
					}
				}
				PayS = num;
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
				if (PayS < 1)
				{
					typErr.errCode = 1009;
					typErr.errStr = "Помилка! Немає інформації щодо платежів. Таблиця PayForms.";
					result = typErr;
					goto IL_01a2;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typErr.errStr = "Помилка обробки даних таліці PayForms.";
				typErr.errCode = 17;
				result = typErr;
				ProjectData.ClearProjectError();
				goto IL_01a2;
			}
			result = typErr;
			goto IL_01a2;
		}
		IL_01a2:
		return result;
	}

	internal int AddPayTemp(string NamePay, string GroupPay = "0")
	{
		int result;
		checked
		{
			try
			{
				PayS++;
				ref string[,] pay = ref Pay;
				pay = (string[,])Utils.CopyArray(pay, new string[3, PayS + 1]);
				Pay[0, PayS] = NamePay;
				Pay[1, PayS] = GroupPay;
				Pay[2, PayS] = "0";
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = -1;
				ProjectData.ClearProjectError();
				goto IL_0086;
			}
			result = PayS;
			goto IL_0086;
		}
		IL_0086:
		return result;
	}

	private TypErr LoadTaxObjeckt()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		TypErrTaxObj typErrTaxObj = All.l.InfaTaxObjects();
		if (typErrTaxObj.errCode > 0)
		{
			result.errCode = typErrTaxObj.errCode;
			result.errStr = typErrTaxObj.errStr;
			return result;
		}
		All.A.TIN = typErrTaxObj.tTIN;
		All.A.INN = typErrTaxObj.tINN;
		All.A.PointAddr = typErrTaxObj.tPointAddr;
		All.A.PointName = typErrTaxObj.tPointName;
		All.A.OrgName = typErrTaxObj.tOrgName;
		return result;
	}

	public TypErrTaxABC Search(string NameTaxe)
	{
		TypErrTaxABC result = default(TypErrTaxABC);
		result.errCode = 0;
		result.errStr = "";
		result.TaxExCiSe = 0.0;
		result.TaxIndex = 0;
		result.TaxName = "";
		result.TaxPrc = 0.0;
		int num = 0;
		int taxS = TaxS;
		for (num = 1; num <= taxS; num = checked(num + 1))
		{
			if (Operators.CompareString(Tax[0, num].Trim().ToLower(), NameTaxe.Trim().ToLower(), TextCompare: false) == 0)
			{
				result.TaxExCiSe = All.StrToDouble(Tax[1, num]);
				result.TaxIndex = num;
				result.TaxName = Tax[0, num];
				result.TaxPrc = All.StrToDouble(Tax[2, num]);
				return result;
			}
		}
		result.errCode = 1008;
		result.errStr = "Помилка. Не можу знайти інформацію про податок.";
		return result;
	}

	public TypErrTaxABC SearchTaxPr(string PrTaxe)
	{
		TypErrTaxABC result = default(TypErrTaxABC);
		result.errCode = 0;
		result.errStr = "";
		result.TaxExCiSe = 0.0;
		result.TaxIndex = 0;
		result.TaxName = "";
		result.TaxPrc = 0.0;
		int num = 0;
		int taxS = TaxS;
		for (num = 1; num <= taxS; num = checked(num + 1))
		{
			if (Operators.CompareString(Tax[2, num].Trim().ToLower(), PrTaxe.Trim().ToLower(), TextCompare: false) == 0)
			{
				result.TaxExCiSe = Conversions.ToInteger(Tax[1, num]);
				result.TaxIndex = num;
				result.TaxName = Tax[0, num];
				result.TaxPrc = All.StrToDouble(Tax[2, num]);
				return result;
			}
		}
		result.errCode = 1008;
		result.errStr = "Не можу знайти інформацію про податок '" + PrTaxe + "'";
		return result;
	}

	public void ZeroTax()
	{
		int taxS = TaxS;
		checked
		{
			for (int i = 0; i <= taxS; i++)
			{
				int num = 0;
				do
				{
					ReportTax[num, i] = "";
					num++;
				}
				while (num <= 9);
			}
			int taxS2 = TaxS;
			for (int i = 0; i <= taxS2; i++)
			{
				Tax[3, i] = "0";
				Tax[4, i] = "0";
			}
			int payS = PayS;
			for (int i = 0; i <= payS; i++)
			{
				Pay[2, i] = "0";
			}
		}
	}

	public string NamePayToGroup(string NamePay)
	{
		if (Operators.CompareString(NamePay.ToLower().Trim(), "готівка", TextCompare: false) == 0)
		{
			return "ГОТІВКА";
		}
		return "БЕЗГОТІВКОВА";
	}

	public int NamePayToIndex(string eNamePay, bool Synonyms = true)
	{
		eNamePay = eNamePay.Trim().ToLower();
		if (Synonyms)
		{
			if (Operators.CompareString(eNamePay, "", TextCompare: false) == 0)
			{
				return 999999999;
			}
			if (SearchSynonyms(eNamePay))
			{
				return 999999999;
			}
		}
		int payS = PayS;
		for (int i = 1; i <= payS; i = checked(i + 1))
		{
			if (Operators.CompareString(Pay[0, i].ToLower(), eNamePay, TextCompare: false) == 0)
			{
				return i;
			}
		}
		return -1;
	}

	private bool SearchSynonyms(string NameS)
	{
		NameS = NameS.ToLower();
		return NameS switch
		{
			"готивка" => true, 
			"готiвка" => true, 
			"гoтiвка" => true, 
			"гoтiвкa" => true, 
			"гoтiвkа" => true, 
			"gotivka" => true, 
			"нал" => true, 
			"наличка" => true, 
			"налич" => true, 
			"нaл" => true, 
			"готівка" => true, 
			"гoтівка" => true, 
			"готівкa" => true, 
			"гoтівкa" => true, 
			"nal" => true, 
			"nalichka" => true, 
			_ => false, 
		};
	}
}
