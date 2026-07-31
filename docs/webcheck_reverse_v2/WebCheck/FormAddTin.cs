using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
public class FormAddTin : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("SelSwrver")]
	private Button _SelSwrver;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("ImportDat")]
	private Button _ImportDat;

	[CompilerGenerated]
	[AccessedThroughProperty("ButtonOk")]
	private Button _ButtonOk;

	[CompilerGenerated]
	[AccessedThroughProperty("ButtonCancel")]
	private Button _ButtonCancel;

	private AccountantОffice AO;

	[field: AccessedThroughProperty("Server")]
	internal virtual TextBox Server
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SelSwrver
	{
		[CompilerGenerated]
		get
		{
			return _SelSwrver;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click -= value2;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click += value2;
			}
		}
	}

	internal virtual Button KeyB
	{
		[CompilerGenerated]
		get
		{
			return _KeyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click -= value2;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label11")]
	internal virtual Label Label11
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PasOpT")]
	internal virtual TextBox PasOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KeyOpT")]
	internal virtual TextBox KeyOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label21")]
	internal virtual Label Label21
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ImportDat
	{
		[CompilerGenerated]
		get
		{
			return _ImportDat;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ImportDat_Click;
			Button importDat = _ImportDat;
			if (importDat != null)
			{
				importDat.Click -= value2;
			}
			_ImportDat = value;
			importDat = _ImportDat;
			if (importDat != null)
			{
				importDat.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxTin")]
	internal virtual TextBox TextBoxTin
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ButtonOk
	{
		[CompilerGenerated]
		get
		{
			return _ButtonOk;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ButtonOk_Click;
			Button buttonOk = _ButtonOk;
			if (buttonOk != null)
			{
				buttonOk.Click -= value2;
			}
			_ButtonOk = value;
			buttonOk = _ButtonOk;
			if (buttonOk != null)
			{
				buttonOk.Click += value2;
			}
		}
	}

	internal virtual Button ButtonCancel
	{
		[CompilerGenerated]
		get
		{
			return _ButtonCancel;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ButtonCancel_Click;
			Button buttonCancel = _ButtonCancel;
			if (buttonCancel != null)
			{
				buttonCancel.Click -= value2;
			}
			_ButtonCancel = value;
			buttonCancel = _ButtonCancel;
			if (buttonCancel != null)
			{
				buttonCancel.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormAddTin));
		this.Server = new System.Windows.Forms.TextBox();
		this.SelSwrver = new System.Windows.Forms.Button();
		this.KeyB = new System.Windows.Forms.Button();
		this.Label11 = new System.Windows.Forms.Label();
		this.Label10 = new System.Windows.Forms.Label();
		this.PasOpT = new System.Windows.Forms.TextBox();
		this.KeyOpT = new System.Windows.Forms.TextBox();
		this.Label21 = new System.Windows.Forms.Label();
		this.ImportDat = new System.Windows.Forms.Button();
		this.TextBoxTin = new System.Windows.Forms.TextBox();
		this.ButtonOk = new System.Windows.Forms.Button();
		this.ButtonCancel = new System.Windows.Forms.Button();
		this.Label1 = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.Server.Enabled = false;
		this.Server.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Server.Location = new System.Drawing.Point(300, 122);
		this.Server.Name = "Server";
		this.Server.Size = new System.Drawing.Size(402, 30);
		this.Server.TabIndex = 32;
		this.Server.TabStop = false;
		this.Server.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.SelSwrver.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SelSwrver.Location = new System.Drawing.Point(721, 122);
		this.SelSwrver.Name = "SelSwrver";
		this.SelSwrver.Size = new System.Drawing.Size(53, 30);
		this.SelSwrver.TabIndex = 31;
		this.SelSwrver.Text = "...";
		this.SelSwrver.UseVisualStyleBackColor = true;
		this.KeyB.Location = new System.Drawing.Point(721, 28);
		this.KeyB.Name = "KeyB";
		this.KeyB.Size = new System.Drawing.Size(53, 30);
		this.KeyB.TabIndex = 25;
		this.KeyB.Text = "...";
		this.KeyB.UseVisualStyleBackColor = true;
		this.Label11.AutoSize = true;
		this.Label11.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label11.Location = new System.Drawing.Point(22, 79);
		this.Label11.Name = "Label11";
		this.Label11.Size = new System.Drawing.Size(202, 25);
		this.Label11.TabIndex = 30;
		this.Label11.Text = "Пароль ключа ЕЦП *";
		this.Label10.AutoSize = true;
		this.Label10.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label10.Location = new System.Drawing.Point(22, 33);
		this.Label10.Name = "Label10";
		this.Label10.Size = new System.Drawing.Size(121, 25);
		this.Label10.TabIndex = 29;
		this.Label10.Text = "Ключ ЕЦП *";
		this.PasOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PasOpT.Location = new System.Drawing.Point(300, 76);
		this.PasOpT.Name = "PasOpT";
		this.PasOpT.PasswordChar = '*';
		this.PasOpT.Size = new System.Drawing.Size(402, 30);
		this.PasOpT.TabIndex = 28;
		this.PasOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.KeyOpT.Enabled = false;
		this.KeyOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.KeyOpT.Location = new System.Drawing.Point(300, 30);
		this.KeyOpT.Name = "KeyOpT";
		this.KeyOpT.Size = new System.Drawing.Size(402, 30);
		this.KeyOpT.TabIndex = 27;
		this.KeyOpT.TabStop = false;
		this.KeyOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label21.AutoSize = true;
		this.Label21.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label21.Location = new System.Drawing.Point(22, 125);
		this.Label21.Name = "Label21";
		this.Label21.Size = new System.Drawing.Size(77, 25);
		this.Label21.TabIndex = 26;
		this.Label21.Text = "АЦСК *";
		this.ImportDat.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ImportDat.Location = new System.Drawing.Point(27, 175);
		this.ImportDat.Name = "ImportDat";
		this.ImportDat.Size = new System.Drawing.Size(747, 48);
		this.ImportDat.TabIndex = 34;
		this.ImportDat.Text = "Завантаження даних з кабінету податкової...";
		this.ImportDat.UseVisualStyleBackColor = true;
		this.TextBoxTin.BackColor = System.Drawing.SystemColors.Window;
		this.TextBoxTin.Enabled = false;
		this.TextBoxTin.Font = new System.Drawing.Font("Microsoft Sans Serif", 16.2f, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBoxTin.ForeColor = System.Drawing.SystemColors.WindowText;
		this.TextBoxTin.Location = new System.Drawing.Point(27, 267);
		this.TextBoxTin.Multiline = true;
		this.TextBoxTin.Name = "TextBoxTin";
		this.TextBoxTin.ReadOnly = true;
		this.TextBoxTin.Size = new System.Drawing.Size(747, 180);
		this.TextBoxTin.TabIndex = 35;
		this.TextBoxTin.TabStop = false;
		this.TextBoxTin.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.ButtonOk.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ButtonOk.Location = new System.Drawing.Point(419, 472);
		this.ButtonOk.Name = "ButtonOk";
		this.ButtonOk.Size = new System.Drawing.Size(355, 48);
		this.ButtonOk.TabIndex = 36;
		this.ButtonOk.Text = "Підключити";
		this.ButtonOk.UseVisualStyleBackColor = true;
		this.ButtonCancel.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ButtonCancel.Location = new System.Drawing.Point(27, 472);
		this.ButtonCancel.Name = "ButtonCancel";
		this.ButtonCancel.Size = new System.Drawing.Size(355, 48);
		this.ButtonCancel.TabIndex = 37;
		this.ButtonCancel.Text = "Відміна";
		this.ButtonCancel.UseVisualStyleBackColor = true;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(22, 239);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(316, 25);
		this.Label1.TabIndex = 38;
		this.Label1.Text = "Результат завантаження даних:";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(804, 547);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.ButtonCancel);
		base.Controls.Add(this.ButtonOk);
		base.Controls.Add(this.TextBoxTin);
		base.Controls.Add(this.ImportDat);
		base.Controls.Add(this.Server);
		base.Controls.Add(this.SelSwrver);
		base.Controls.Add(this.KeyB);
		base.Controls.Add(this.Label11);
		base.Controls.Add(this.Label10);
		base.Controls.Add(this.PasOpT);
		base.Controls.Add(this.KeyOpT);
		base.Controls.Add(this.Label21);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormAddTin";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Завантаження даних про підприємство";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormAddTin(bool e = true)
	{
		base.Load += FormAddTin_Load;
		AO = new AccountantОffice();
		InitializeComponent();
		if (!e)
		{
			ButtonOk.Text = "Оновити дані";
			Text = "Завантаження даних з кабінету податкової для оновлення...";
		}
	}

	private void FormAddTin_Load(object sender, EventArgs e)
	{
		All.TinBux = "";
		ButtonOk.Enabled = false;
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string text = PathKey();
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			return;
		}
		KeyOpT.Text = text;
		string left = KeyTip(text);
		if (Operators.CompareString(left, "zs2", TextCompare: false) != 0)
		{
			if (Operators.CompareString(left, "jks", TextCompare: false) == 0)
			{
				All.A.AcskSettingsTemp = 4;
				Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
				All.A.AcskSettings = All.A.AcskSettingsTemp;
				Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
			}
		}
		else
		{
			All.A.AcskSettingsTemp = 2;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			All.A.AcskSettings = All.A.AcskSettingsTemp;
			Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
		}
		PasOpT.Focus();
	}

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		FormServerSelection formServerSelection = new FormServerSelection(NewBase: true);
		formServerSelection.ShowDialog();
		formServerSelection.Dispose();
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
	}

	private void ImportDat_Click(object sender, EventArgs e)
	{
		All.SF.SignatureStart();
		if ((Operators.CompareString(KeyOpT.Text.Trim(), "", TextCompare: false) == 0) | (Operators.CompareString(PasOpT.Text.Trim(), "", TextCompare: false) == 0))
		{
			Interaction.MsgBox("Обов'язкові поля для завантаження даних з кабінету податкової:\r\n- Ключ ЕЦП\r\n- Пароль до ключа ЕЦП\r\n- АЦСК", MsgBoxStyle.Exclamation, "Завантаження даних");
			ButtonOk.Enabled = false;
			return;
		}
		string text = All.MyDoc() + "\\WebCheck\\Temp\\objects.txt";
		if (File.Exists(text))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
		}
		if (File.Exists(text + ".p7s"))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text + ".p7s");
		}
		if (!AO.FileForSend(text))
		{
			TextBoxTin.Text = "Помилка створення файлу!";
			ButtonOk.Enabled = false;
			return;
		}
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		All.SF.SetServer();
		All.RetriesPrt = 3;
		All.SF.ErrorShow(ShowWindows: true);
		if (All.SF.SignatureFile(KeyOpT.Text.Trim(), PasOpT.Text.Trim(), text).errCode > 0)
		{
			TextBoxTin.Text = "Помилка підпису!";
			ButtonOk.Enabled = false;
			return;
		}
		All.SF.ErrorShow(ShowWindows: false);
		All.SF.SetServer();
		string text2 = AO.SendFile(text + ".p7s");
		if (Operators.CompareString(text2.Trim(), "", TextCompare: false) == 0)
		{
			TextBoxTin.Text = "Помилка завантаження даних з кабінета податкової - перевірьте пароль ключа ЕЦП - спробуйте увійти в кабінет податкової з цим ключем ,  перевірити список доступних ПРРО і касирів.";
			ButtonOk.Enabled = false;
			return;
		}
		All.LgAll.SaveTextToLogAll("ADD TIN", text2);
		if (AO.Dereban(text2))
		{
			string text3 = AO.ControlTin(KeyOpT.Text.Trim(), PasOpT.Text.Trim());
			if (Operators.CompareString(text3, AO.InfaTin.TIN, TextCompare: false) != 0)
			{
				TextBoxTin.Text = "Для завантаження даних використовуйте ЕЦП власника ПРРО.";
				All.LgAll.SaveTextToLogAll("ErrorControlTIN", text3 + " - " + AO.InfaTin.TIN);
				ButtonOk.Enabled = false;
				return;
			}
			TextBoxTin.Text = "ЕДРОПУ/ІНН :   " + AO.InfaTin.TIN + "   " + AO.InfaTin.OrgName + "   Кількість фіскальних номерів   " + AO.InfaC;
			ButtonOk.Enabled = true;
		}
		else
		{
			TextBoxTin.Text = "Помилка завантаження даних з кабінета податкової - перевірьте пароль ключа ЕЦП - спробуйте увійти в кабінет податкової з цим ключем ,  перевірити список доступних ПРРО і касирів.";
			ButtonOk.Enabled = false;
		}
	}

	private void ButtonCancel_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void ButtonOk_Click(object sender, EventArgs e)
	{
		if (AO.InfaC <= 0)
		{
			return;
		}
		All.ArS.AddFn(AO.InfaTin.TIN);
		All.ArS.StringWriteFN(AO.InfaTin.TIN, "TIN", AO.InfaTin.TIN);
		All.ArS.StringWriteFN(AO.InfaTin.TIN, "OrgName", AO.InfaTin.OrgName);
		All.ArS.StringWriteFN(AO.InfaTin.TIN, "Address", AO.InfaTin.Address);
		checked
		{
			int num = AO.InfaC - 1;
			for (int i = 0; i <= num; i++)
			{
				CreatingFolder(AO.Infa[i].NumFiscal);
				try
				{
					string sourceFileName = All.MyDoc() + "\\WebCheck\\Temp\\objects.txt.p7s";
					string destinationFileName = All.MyDoc() + "\\WebCheck\\Archive\\All\\" + AO.NameFileTIN(AO.InfaTin.TIN) + ".p7s";
					MyProject.Computer.FileSystem.CopyFile(sourceFileName, destinationFileName, overwrite: true);
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					ProjectData.ClearProjectError();
				}
				int num2 = AO.IndexKeysTin(AO.InfaTin.TIN, AO.Infa[i].NumFiscal);
				if (num2 < 0)
				{
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Fn", Conversions.ToInteger(i.ToString())), AO.Infa[i].NumFiscal);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Ad", Conversions.ToInteger(i.ToString())), AO.Infa[i].Address);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Na", Conversions.ToInteger(i.ToString())), AO.Infa[i].Name);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Or", Conversions.ToInteger(i.ToString())), AO.Infa[i].OrgName);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Up", Conversions.ToInteger(i.ToString())), "");
				}
				else
				{
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Ad", Conversions.ToInteger(num2.ToString())), AO.Infa[i].Address);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Na", Conversions.ToInteger(num2.ToString())), AO.Infa[i].Name);
					All.ArS.StringWriteFN(AO.InfaTin.TIN, AO.NameKeyINI("Or", Conversions.ToInteger(num2.ToString())), AO.Infa[i].OrgName);
				}
			}
			Interaction.MsgBox("Вдало підключено " + AO.InfaTin.OrgName, MsgBoxStyle.OkOnly, AO.InfaTin.TIN);
			All.TinBux = AO.InfaTin.TIN + "   -   " + AO.InfaTin.OrgName;
			Close();
		}
	}

	private string JsonTest()
	{
		return "{'TaxObjects':[{'Entity':34562,'TaxObjGuid':'AF5689C1A57E03D8E0530A21420720FC','TaxObjId':32100001,'SingleTax':false,'Name':'ФОП СУХЕНКО ОЛЕНА СТАНІСЛАВІВНА','Address':'УКРАЇНА, М.КИЇВ ПОДІЛЬСЬКИЙ Р-Н, пр. Правди 10 В кв 141','Tin':'30273038877799','Ipn':null,'OrgName':'СУХЕНКО ОЛЕНА СТАНІСЛАВІВНА','ChiefCashier':false,'TransactionsRegistrars':[{'NumFiscal':4000034080,'NumLocal':1,'Name':'ВебЧек','Closed':false}]}],'UID':null,'Timestamp':'2024-02-26T17:47:06.9069165+02:00'}";
	}

	private string PathKey()
	{
		OpenFileDialog openFileDialog = new OpenFileDialog();
		openFileDialog.Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if (openFileDialog.ShowDialog() == DialogResult.OK)
		{
			return openFileDialog.FileName;
		}
		return "";
	}

	private string KeyTip(string FilePath)
	{
		FilePath = FilePath.Trim();
		string text = "";
		checked
		{
			try
			{
				text = Conversions.ToString(FilePath[FilePath.Trim().Length - 3]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 2]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 1]);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				text = "";
				ProjectData.ClearProjectError();
			}
			return text.ToLower();
		}
	}

	private void CreatingFolder(string NameF)
	{
		if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + NameF + "\\"))
		{
			Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Archive\\" + NameF + "\\");
		}
	}
}
